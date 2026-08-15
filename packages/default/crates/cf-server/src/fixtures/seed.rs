//! Seed the database from the design fixture JSON.
//!
//! Reads `crystal-forge.fixtures.json` and inserts data into the
//! normal application tables (`environments`, `flakes`, `systems`,
//! `system_states`, `agent_heartbeats`, `cves`, `deployment_policies`,
//! `builders`, `build_jobs`, etc.).
//!
//! After seeding, the regular server handlers query the database and
//! return genuine API responses — no middleware interception needed.

use anyhow::{Context, Result};
use serde::Deserialize;
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

use crate::compliance::framework_model::FrameworkVersionCanonical;
use crate::compliance::requirement_model::RequirementVersionCanonical;
use crate::queries::framework_requirements::{
    insert_framework_version_with_requirement_digests, insert_requirement_version,
    upsert_framework_lineage, upsert_requirement_lineage,
};

// ---------------------------------------------------------------------------
// Fixture JSON top-level structure
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FixtureRoot {
    #[serde(rename = "_meta")]
    meta: Option<Meta>,
    environments: Vec<FixtureEnvironment>,
    flakes: FixtureFlakes,
    systems: Vec<FixtureSystem>,
    builds: FixtureBuilds,
    evaluations: FixtureEvaluations,
    cves: FixtureCves,
    policies: Vec<FixturePolicy>,
    compliance: Vec<FixtureCompliance>,
    caches: Vec<serde_json::Value>,
    scanning: serde_json::Value,
    admin: FixtureAdmin,
    hardening: Vec<FixtureHardening>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Meta {
    generated: Option<String>,
    source: Option<String>,
    rng_seed: Option<u64>,
    note: Option<String>,
}

// ---------------------------------------------------------------------------
// Environment
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FixtureEnvironment {
    name: String,
    color: Option<String>,
    dot: Option<String>,
}

// ---------------------------------------------------------------------------
// Flakes
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FixtureFlakes {
    registry: Vec<FixtureFlakeRegistry>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FixtureFlakeRegistry {
    id: String,
    name: String,
    url: String,
    branch: Option<String>,
    description: Option<String>,
    environment: Option<String>,
    system_count: Option<i32>,
    #[serde(alias = "lastSyncAt")]
    last_sync_at: Option<String>,
    status: Option<String>,
    latest_commit: Option<String>,
    latest_message: Option<String>,
    latest_author: Option<String>,
    latest_at: Option<String>,
    total_commits: Option<i32>,
    #[serde(alias = "errorMsg")]
    error_msg: Option<String>,
    commits: Option<Vec<FixtureCommit>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FixtureCommit {
    hash: Option<String>,
    message: Option<String>,
    author: Option<String>,
    committed_at: Option<String>,
    system_count: Option<i32>,
    total_derivations: Option<i32>,
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FixtureSystem {
    id: String,
    hostname: String,
    fqdn: Option<String>,
    environment: String,
    flake: Option<String>,
    branch: Option<String>,
    commit: Option<String>,
    commit_message: Option<String>,
    health: Option<String>,
    status: Option<String>,
    status_color: Option<String>,
    status_chip: Option<String>,
    deployment_policy: Option<String>,
    deployment_state: Option<String>,
    last_heartbeat: Option<String>,
    heartbeat_age: Option<i32>,
    heartbeat_interval_sec: Option<i32>,
    heartbeat_next_in_sec: Option<i32>,
    generation: Option<i32>,
    nixos_version: Option<String>,
    kernel: Option<String>,
    store_path: Option<String>,
    target_store_path: Option<String>,
    uptime: Option<String>,
    cpu: Option<String>,
    mem_gb: Option<f64>,
    ipv4: Option<String>,
    ipv6: Option<String>,
    reachability: Option<String>,
    cves: Option<FixtureSystemCves>,
    tags: Option<Vec<String>>,
    stig: Option<i32>,
    events: Option<Vec<FixtureSystemEvent>>,
    #[serde(rename = "pendingDeployment")]
    pending_deployment: Option<FixturePendingDeployment>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
struct FixturePendingDeployment {
    target_store_path: String,
    source: Option<String>,
    status: Option<String>,
    issued_at: Option<String>,
    delivered_at: Option<String>,
    applying_at: Option<String>,
    completed_at: Option<String>,
    target_generation: Option<i32>,
    target_commit: Option<String>,
    kind: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
struct FixtureSystemEvent {
    at: Option<String>,
    title: Option<String>,
    color: Option<String>,
    event_type: Option<String>,
    outcome: Option<String>,
    source: Option<String>,
    generation: Option<i64>,
    store_path: Option<String>,
    commit_hash: Option<String>,
    actor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FixtureSystemCves {
    critical: Option<i32>,
    high: Option<i32>,
    medium: Option<i32>,
    low: Option<i32>,
    total: Option<i32>,
}

// ---------------------------------------------------------------------------
// Builds
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FixtureBuilds {
    active: Vec<FixtureBuildItem>,
    history: Vec<FixtureBuildItem>,
    stats: serde_json::Value,
    workers: Vec<FixtureWorker>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FixtureBuildItem {
    id: String,
    system: Option<String>,
    name: Option<String>,
    flake: Option<String>,
    drv: Option<String>,
    commit: Option<String>,
    status: Option<String>,
    meta: Option<serde_json::Value>,
    worker: Option<String>,
    arch: Option<String>,
    total_derivs: Option<i32>,
    built_derivs: Option<i32>,
    cached_derivs: Option<i32>,
    current_pkg: Option<String>,
    queued_at: Option<String>,
    dur: Option<String>,
    progress: Option<f64>,
    attempts: Option<i32>,
    log_lines: Option<i32>,
    failed_pkg: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FixtureWorker {
    id: String,
    fingerprint: Option<String>,
    registered: Option<bool>,
    name: String,
    host: Option<String>,
    arch: Option<String>,
    cores: Option<i32>,
    mem: Option<i32>,
    slots: Option<serde_json::Value>,
    status: Option<String>,
    load: Option<f64>,
    last_seen: Option<String>,
    uptime_days: Option<i32>,
    completed_24h: Option<i32>,
    failed_24h: Option<i32>,
    environments: Option<Vec<String>>,
    public_key: Option<String>,
}

// ---------------------------------------------------------------------------
// Evaluations
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FixtureEvaluations {
    active: Vec<serde_json::Value>,
    history: Vec<serde_json::Value>,
    stats: serde_json::Value,
}

// ---------------------------------------------------------------------------
// CVEs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FixtureCves {
    list: Vec<FixtureCveItem>,
    stats: serde_json::Value,
    insights: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FixtureCveItem {
    id: String,
    pkg: Option<String>,
    severity: Option<String>,
    cvss: Option<f64>,
    title: Option<String>,
    introduced_in: Option<String>,
    fixed_in: Option<String>,
    fix: Option<String>,
    age_days: Option<i32>,
    exploited: Option<bool>,
    affected: Option<Vec<String>>,
    affected_count: Option<i32>,
    advisory_url: Option<String>,
    vector: Option<String>,
    discovered_at: Option<String>,
    acceptance: Option<String>,
    justification: Option<String>,
    justified_by: Option<String>,
    justified_at: Option<String>,
}

// ---------------------------------------------------------------------------
// Policies
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FixturePolicy {
    id: String,
    name: String,
    category: Option<String>,
    description: Option<String>,
    #[serde(rename = "type")]
    policy_type: Option<String>,
    rules: Option<Vec<serde_json::Value>>,
    rationale: Option<String>,
}

// ---------------------------------------------------------------------------
// Normalized compliance fixtures
// ---------------------------------------------------------------------------

/// The existing compliance entries are design-oriented bundle summaries.  A
/// small optional normalized shape is accepted alongside them so the fast UI
/// harness can seed real framework/requirement API data without coupling the
/// browser test to a database-only setup script.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FixtureCompliance {
    #[serde(rename = "canonicalSourceKey")]
    canonical_source_key: Option<String>,
    name: Option<String>,
    publisher: Option<String>,
    description: Option<String>,
    versions: Option<Vec<FixtureComplianceVersion>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FixtureComplianceVersion {
    version: String,
    #[serde(rename = "canonicalReleaseKey")]
    canonical_release_key: String,
    title: Option<String>,
    #[serde(rename = "publishedAt")]
    published_at: Option<String>,
    #[serde(default)]
    requirements: Vec<FixtureComplianceRequirement>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FixtureComplianceRequirement {
    #[serde(rename = "canonicalRequirementKey")]
    canonical_requirement_key: String,
    #[serde(rename = "externalId")]
    external_id: String,
    title: Option<String>,
    description: Option<String>,
    kind: String,
    severity: Option<String>,
    #[serde(rename = "checkText")]
    check_text: Option<String>,
    #[serde(rename = "fixText")]
    fix_text: Option<String>,
    metadata: Option<serde_json::Value>,
    #[serde(rename = "parentExternalId")]
    parent_external_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Admin
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FixtureAdmin {
    users: Vec<FixtureUser>,
    oidc_mappings: Option<Vec<serde_json::Value>>,
    roles: Option<Vec<serde_json::Value>>,
    audit_log: Option<Vec<serde_json::Value>>,
    server: Option<serde_json::Value>,
    background_jobs: Option<Vec<serde_json::Value>>,
    heartbeat: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FixtureUser {
    id: String,
    name: Option<String>,
    email: String,
    role: Option<String>,
    source: Option<String>,
    groups: Option<Vec<String>>,
    envs: Option<Vec<String>>,
    status: Option<String>,
    last_login: Option<String>,
    mfa: Option<bool>,
}

// ---------------------------------------------------------------------------
// Hardening
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FixtureHardening {
    id: String,
    name: String,
    score: Option<i32>,
    risk: Option<String>,
    risk_color: Option<String>,
    enabled: Option<Vec<bool>>,
    missing: Option<i32>,
    nix_snippet: Option<String>,
    user: Option<String>,
    notes: Option<i32>,
}

// ---------------------------------------------------------------------------
// Main entry: seed database from fixture JSON file
// ---------------------------------------------------------------------------

/// Seed the database from a fixture JSON file.
///
/// This is called at server startup when `FIXTURE_JSON_PATH` is set.
/// It inserts the fixture data into the appropriate application tables
/// so that the regular API handlers can serve it.
pub async fn seed_from_fixture(pool: &PgPool, path: &Path) -> Result<()> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read fixture file: {}", path.display()))?;
    let fixture: FixtureRoot = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse fixture file: {}", path.display()))?;

    tracing::info!(
        "Seeding database from fixture: {} ({} systems, {} builds, {} CVEs, {} environments ...)",
        path.display(),
        fixture.systems.len(),
        fixture.builds.active.len() + fixture.builds.history.len(),
        fixture.cves.list.len(),
        fixture.environments.len(),
    );

    // Order matters for FK references:
    // 1. Environments (referenced by systems, build_jobs)
    // 2. Flakes (referenced by systems, commits)
    // 3. Commits (referenced by evaluations, derivations)
    // 4. Deployment policies (referenced by systems)
    // 5. Users (referenced by identities)
    // 6. Systems (referenced by system_states)
    // 7. System states (referenced by agent_heartbeats)
    // 8. Agent heartbeats
    // 9. CVEs + package_vulnerabilities
    // 10. Builders + build_jobs
    // 11. Hardening scans + results

    seed_environments(pool, &fixture.environments).await?;
    let flake_ids = seed_flakes(pool, &fixture.flakes.registry).await?;
    seed_commits(pool, &fixture.flakes.registry, &flake_ids).await?;
    let policy_ids = seed_deployment_policies(pool, &fixture.policies).await?;
    seed_compliance_frameworks(pool, &fixture.compliance).await?;
    let _user_ids = seed_users(pool, &fixture.admin).await?;
    let system_ids = seed_systems(pool, &fixture.systems, &flake_ids, &policy_ids).await?;
    seed_system_states(pool, &fixture.systems, &system_ids).await?;
    seed_system_events_and_pending_deployments(pool, &fixture.systems, &system_ids).await?;
    seed_cves(pool, &fixture.cves, &fixture.systems, &system_ids).await?;
    // Builders and build jobs are seeded after systems
    seed_builders_and_jobs(pool, &fixture.builds, &fixture.systems, &system_ids).await?;
    seed_hardening(pool, &fixture.hardening, &fixture.systems, &system_ids).await?;

    // Dismiss the onboarding coach for every user. The coach panel uses
    // hardcoded dark inline styles that ignore the light theme, so leaving it
    // visible pollutes light-mode screenshots with a dark overlay. In a
    // preseeded fixture/demo state the onboarding flow is already "done".
    dismiss_onboarding_for_all_users(pool).await?;

    tracing::info!("Fixture seeding complete");
    Ok(())
}

/// Seed the optional normalized framework fixtures used by focused UI checks.
/// Existing design-only compliance entries are ignored when they do not carry
/// a canonical framework key, so this remains backward-compatible with the
/// original fixture format.
async fn seed_compliance_frameworks(pool: &PgPool, fixtures: &[FixtureCompliance]) -> Result<()> {
    for fixture in fixtures {
        let Some(canonical_source_key) = fixture.canonical_source_key.as_deref() else {
            continue;
        };
        let Some(versions) = fixture.versions.as_ref() else {
            continue;
        };

        let mut tx = pool
            .begin()
            .await
            .context("begin compliance fixture transaction")?;
        let framework_id = upsert_framework_lineage(
            &mut tx,
            fixture.name.as_deref().unwrap_or(canonical_source_key),
            fixture.publisher.as_deref(),
            canonical_source_key,
            fixture.description.as_deref(),
        )
        .await
        .context("seed compliance framework lineage")?;

        for version in versions {
            let framework_version_id = existing_or_insert_framework_version(
                &mut tx,
                framework_id,
                canonical_source_key,
                version,
            )
            .await?;

            let mut requirement_versions = HashMap::new();
            for requirement in &version.requirements {
                let requirement_id = upsert_requirement_lineage(
                    &mut tx,
                    framework_id,
                    &requirement.canonical_requirement_key,
                )
                .await
                .context("seed compliance requirement lineage")?;
                let canonical = RequirementVersionCanonical {
                    canonical_requirement_key: requirement.canonical_requirement_key.clone(),
                    external_id: requirement.external_id.clone(),
                    title: requirement.title.clone(),
                    description: requirement.description.clone(),
                    kind: requirement.kind.clone(),
                    severity: requirement.severity.clone(),
                    check_text: requirement.check_text.clone(),
                    fix_text: requirement.fix_text.clone(),
                    metadata: requirement
                        .metadata
                        .clone()
                        .unwrap_or_else(|| serde_json::json!({})),
                };
                let requirement_version_id = insert_requirement_version(
                    &mut tx,
                    requirement_id,
                    framework_version_id,
                    &canonical,
                    None,
                )
                .await
                .context("seed compliance requirement version")?;
                requirement_versions
                    .insert(requirement.external_id.clone(), requirement_version_id);
                requirement_versions.insert(
                    requirement.canonical_requirement_key.clone(),
                    requirement_version_id,
                );
            }

            // Resolve hierarchy after every requirement version has an ID so
            // fixture order cannot drop a child-to-parent relationship.
            for requirement in &version.requirements {
                let Some(parent_key) = requirement.parent_external_id.as_ref() else {
                    continue;
                };
                let Some(parent_id) = requirement_versions.get(parent_key).copied() else {
                    continue;
                };
                let Some(requirement_version_id) =
                    requirement_versions.get(&requirement.external_id).copied()
                else {
                    continue;
                };
                sqlx::query(
                    "UPDATE compliance_requirement_versions\
                     SET parent_requirement_version_id = $1\
                     WHERE id = $2",
                )
                .bind(parent_id)
                .bind(requirement_version_id)
                .execute(&mut *tx)
                .await
                .context("seed compliance requirement hierarchy")?;
            }
        }

        tx.commit()
            .await
            .context("commit compliance fixture transaction")?;
    }
    Ok(())
}

async fn existing_or_insert_framework_version(
    tx: &mut Transaction<'_, Postgres>,
    framework_id: Uuid,
    canonical_source_key: &str,
    version: &FixtureComplianceVersion,
) -> Result<Uuid> {
    if let Some(id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM compliance_framework_versions WHERE framework_id = $1 AND canonical_release_key = $2",
    )
    .bind(framework_id)
    .bind(&version.canonical_release_key)
    .fetch_optional(&mut **tx)
    .await
    .context("check compliance framework version fixture")?
    {
        return Ok(id);
    }

    let published_at = version
        .published_at
        .as_deref()
        .map(chrono::DateTime::parse_from_rfc3339)
        .transpose()
        .context("parse compliance framework fixture publication date")?
        .map(|value| value.with_timezone(&chrono::Utc));
    let canonical = FrameworkVersionCanonical {
        canonical_source_key: canonical_source_key.to_owned(),
        canonical_release_key: version.canonical_release_key.clone(),
        version: version.version.clone(),
        publisher: None,
        title: version.title.clone(),
    };
    let requirement_digests: Vec<String> = version
        .requirements
        .iter()
        .map(|requirement| {
            RequirementVersionCanonical {
                canonical_requirement_key: requirement.canonical_requirement_key.clone(),
                external_id: requirement.external_id.clone(),
                title: requirement.title.clone(),
                description: requirement.description.clone(),
                kind: requirement.kind.clone(),
                severity: requirement.severity.clone(),
                check_text: requirement.check_text.clone(),
                fix_text: requirement.fix_text.clone(),
                metadata: requirement
                    .metadata
                    .clone()
                    .unwrap_or_else(|| serde_json::json!({})),
            }
            .compute_digest()
        })
        .collect();
    insert_framework_version_with_requirement_digests(
        tx,
        framework_id,
        &canonical,
        None,
        published_at,
        &requirement_digests,
    )
    .await
    .context("seed compliance framework version")
}

/// Mark the setup wizard as dismissed and acknowledged for every user so the
/// onboarding coach overlay never appears in fixture screenshots.
async fn dismiss_onboarding_for_all_users(pool: &PgPool) -> Result<()> {
    let affected = sqlx::query(
        r#"
        UPDATE users
        SET setup_wizard_dismissed = TRUE,
            setup_wizard_agent_acknowledged = TRUE
        WHERE setup_wizard_dismissed = FALSE
           OR setup_wizard_agent_acknowledged = FALSE
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to dismiss onboarding coach for users")?
    .rows_affected();

    if affected > 0 {
        tracing::info!("Dismissed onboarding coach for {} user(s)", affected);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Seeded ID maps (for FK lookups during seeding)
// ---------------------------------------------------------------------------

/// Map from environment name -> environment UUID (after seeding).
struct EnvIdMap {
    production: Uuid,
    staging: Uuid,
    lab: Uuid,
    dev: Uuid,
    test: Uuid,
    other: Uuid,
}

impl EnvIdMap {
    /// Lookup by fixture JSON's environment name string.
    async fn from_db(pool: &PgPool) -> Result<Self> {
        let rows: Vec<(Uuid, String)> = sqlx::query_as("SELECT id, name FROM environments")
            .fetch_all(pool)
            .await
            .context("Failed to load environment IDs")?;

        let mut map = std::collections::HashMap::new();
        for (id, name) in rows {
            map.insert(name, id);
        }

        Ok(Self {
            production: map
                .get("production")
                .copied()
                .unwrap_or_else(|| map.values().next().copied().unwrap_or(Uuid::nil())),
            staging: map
                .get("staging")
                .copied()
                .unwrap_or_else(|| map.get("production").copied().unwrap_or(Uuid::nil())),
            lab: map
                .get("lab")
                .copied()
                .unwrap_or_else(|| map.get("staging").copied().unwrap_or(Uuid::nil())),
            dev: map
                .get("dev")
                .copied()
                .unwrap_or_else(|| map.get("lab").copied().unwrap_or(Uuid::nil())),
            test: map
                .get("test")
                .copied()
                .unwrap_or_else(|| map.get("dev").copied().unwrap_or(Uuid::nil())),
            other: map.values().next().copied().unwrap_or(Uuid::nil()),
        })
    }

    fn get(&self, name: &str) -> Uuid {
        match name.to_lowercase().as_str() {
            "production" | "prod" => self.production,
            "staging" | "stage" => self.staging,
            "lab" => self.lab,
            "dev" | "development" => self.dev,
            "test" | "testing" | "qa" => self.test,
            _ => self.other,
        }
    }
}

/// Map from flake name -> flake id (i32).
#[derive(Default)]
struct FlakeIdMap(std::collections::HashMap<String, i32>);

impl FlakeIdMap {
    fn get(&self, name: &str) -> Option<i32> {
        self.0.get(name).copied()
    }
}

/// Map from system fixture id -> system UUID.
#[derive(Default)]
struct SystemIdMap(std::collections::HashMap<String, uuid::Uuid>);

impl SystemIdMap {
    fn insert(&mut self, fixture_id: String, db_id: uuid::Uuid) {
        self.0.insert(fixture_id, db_id);
    }
    fn get(&self, fixture_id: &str) -> Option<uuid::Uuid> {
        self.0.get(fixture_id).copied()
    }
}

// ---------------------------------------------------------------------------
// Seed: environments
// ---------------------------------------------------------------------------

async fn seed_environments(pool: &PgPool, envs: &[FixtureEnvironment]) -> Result<()> {
    if envs.is_empty() {
        return Ok(());
    }

    tracing::info!("Seeding {} environments", envs.len());
    for env in envs {
        let color = env.color.as_deref().unwrap_or("#6B7280");
        sqlx::query(
            r#"
            INSERT INTO environments (name, description, is_active, color_hex)
            VALUES ($1, $2, TRUE, $3)
            ON CONFLICT (name) DO UPDATE SET
                color_hex = EXCLUDED.color_hex
            "#,
        )
        .bind(&env.name)
        .bind(&format!("{} environment (fixture)", env.name))
        .bind(color)
        .execute(pool)
        .await
        .with_context(|| format!("Failed to seed environment '{}'", env.name))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Seed: flakes (tbl_flakes -> flakes)
// ---------------------------------------------------------------------------

async fn seed_flakes(pool: &PgPool, registry: &[FixtureFlakeRegistry]) -> Result<FlakeIdMap> {
    if registry.is_empty() {
        return Ok(FlakeIdMap::default());
    }

    tracing::info!("Seeding {} flakes", registry.len());
    let mut map = FlakeIdMap::default();

    for flake in registry {
        let repo_url = flake
            .url
            .trim_start_matches("git+ssh://")
            .trim_start_matches("https://")
            .to_string();
        let branch = flake.branch.as_deref().unwrap_or("main");
        let sync_status = flake.status.as_deref().unwrap_or("unknown");
        let last_sync_at = flake.last_sync_at.as_deref().and_then(parse_relative_time);
        let last_sync_error = flake.error_msg.as_deref();

        // We need the flake ID for the FK map, so INSERT ... RETURNING id
        let id: i32 = sqlx::query_scalar(
            r#"
            INSERT INTO flakes (name, repo_url, branch, build_scope, sync_status, last_sync_at, last_sync_error)
            VALUES ($1, $2, $3, 'cf_systems_only', $4, $5, $6)
            ON CONFLICT (repo_url) DO UPDATE SET
                name = EXCLUDED.name,
                branch = EXCLUDED.branch,
                sync_status = EXCLUDED.sync_status,
                last_sync_at = EXCLUDED.last_sync_at,
                last_sync_error = EXCLUDED.last_sync_error
            RETURNING id
            "#,
        )
        .bind(&flake.name)
        .bind(&repo_url)
        .bind(branch)
        .bind(sync_status)
        .bind(last_sync_at)
        .bind(last_sync_error)
        .fetch_one(pool)
        .await
        .with_context(|| format!("Failed to seed flake '{}'", flake.name))?;

        map.0.insert(flake.name.clone(), id);
    }

    Ok(map)
}

// ---------------------------------------------------------------------------
// Seed: commits
// ---------------------------------------------------------------------------

async fn seed_commits(
    pool: &PgPool,
    registry: &[FixtureFlakeRegistry],
    flake_ids: &FlakeIdMap,
) -> Result<()> {
    let mut count = 0usize;
    for flake in registry {
        let Some(flake_id) = flake_ids.get(&flake.name) else {
            continue;
        };

        // Create a synthetic commit from the flake's latestCommit metadata
        if let Some(hash) = &flake.latest_commit {
            let message = flake.latest_message.as_deref().unwrap_or("Fixture commit");
            let author = flake.latest_author.as_deref().unwrap_or("fixture");
            let commit_timestamp = sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>(
                "SELECT NOW() - INTERVAL '1 hour'",
            )
            .fetch_one(pool)
            .await?;

            sqlx::query(
                r#"
                INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, message, author)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT (flake_id, git_commit_hash) DO NOTHING
                "#,
            )
            .bind(flake_id)
            .bind(hash)
            .bind(commit_timestamp)
            .bind(message)
            .bind(author)
            .execute(pool)
            .await
            .with_context(|| format!("Failed to seed commit for flake '{}'", flake.name))?;
            count += 1;
        }

        // Also try commits array if present
        if let Some(commits) = &flake.commits {
            for commit in commits {
                let hash = commit.hash.as_deref().unwrap_or("0000000");
                let ts = sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>(
                    "SELECT NOW() - (random() * INTERVAL '30 days')",
                )
                .fetch_one(pool)
                .await?;

                sqlx::query(
                    r#"
                    INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, message, author)
                    VALUES ($1, $2, $3, $4, $5)
                    ON CONFLICT (flake_id, git_commit_hash) DO NOTHING
                    "#,
                )
                .bind(flake_id)
                .bind(hash)
                .bind(ts)
                .bind(commit.message.as_deref().unwrap_or(""))
                .bind(commit.author.as_deref().unwrap_or("fixture"))
                .execute(pool)
                .await?;
                count += 1;
            }
        }
    }

    tracing::info!("Seeded {} commits", count);
    Ok(())
}

// ---------------------------------------------------------------------------
// Seed: deployment policies
// ---------------------------------------------------------------------------

async fn seed_deployment_policies(
    pool: &PgPool,
    policies: &[FixturePolicy],
) -> Result<Vec<String>> {
    if policies.is_empty() {
        return Ok(Vec::new());
    }

    tracing::info!("Seeding {} deployment policies", policies.len());
    let mut names = Vec::new();

    for policy in policies {
        let config = serde_json::json!({});
        let policy_type = policy.policy_type.as_deref().unwrap_or("manual");

        sqlx::query(
            r#"
            INSERT INTO deployment_policies (name, description, policy_type, config, enabled)
            VALUES ($1, $2, $3, $4, TRUE)
            ON CONFLICT (name) DO UPDATE SET
                description = EXCLUDED.description,
                policy_type = EXCLUDED.policy_type
            "#,
        )
        .bind(&policy.name)
        .bind(policy.description.as_deref().unwrap_or(""))
        .bind(policy_type)
        .bind(&config)
        .execute(pool)
        .await
        .with_context(|| format!("Failed to seed policy '{}'", policy.name))?;

        names.push(policy.name.clone());
    }

    Ok(names)
}

// ---------------------------------------------------------------------------
// Seed: users + external identities
// ---------------------------------------------------------------------------

async fn seed_users(pool: &PgPool, admin: &FixtureAdmin) -> Result<Vec<(String, Uuid)>> {
    if admin.users.is_empty() {
        return Ok(Vec::new());
    }

    tracing::info!("Seeding {} users", admin.users.len());
    let mut user_ids = Vec::new();

    for user in &admin.users {
        let name_parts: Vec<&str> = user
            .name
            .as_deref()
            .unwrap_or("Fixture User")
            .splitn(2, ' ')
            .collect();
        let first_name = name_parts.first().copied().unwrap_or("Fixture");
        let last_name = name_parts.get(1).copied().unwrap_or("User");

        // Check if user already exists by email
        let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
            .bind(&user.email)
            .fetch_optional(pool)
            .await?;

        let user_id = if let Some(id) = existing {
            // Ensure the onboarding coach stays dismissed for a clean UI.
            sqlx::query(
                r#"
                UPDATE users
                SET setup_wizard_dismissed = TRUE,
                    setup_wizard_agent_acknowledged = TRUE
                WHERE id = $1
                "#,
            )
            .bind(id)
            .execute(pool)
            .await?;
            id
        } else {
            // Seed users with the setup wizard already dismissed/acknowledged so
            // the onboarding coach overlay does not appear in screenshots (it uses
            // hardcoded dark inline styles that ignore the light theme).
            sqlx::query_scalar::<_, Uuid>(
                r#"
                INSERT INTO users
                    (username, first_name, last_name, email, user_type, is_active,
                     setup_wizard_dismissed, setup_wizard_agent_acknowledged)
                VALUES ($1, $2, $3, $4, 'human', TRUE, TRUE, TRUE)
                RETURNING id
                "#,
            )
            .bind(&user.email.split('@').next().unwrap_or("fixture"))
            .bind(first_name)
            .bind(last_name)
            .bind(&user.email)
            .fetch_one(pool)
            .await
            .context("Failed to insert fixture user")?
        };

        // Assign role
        let role = user.role.as_deref().unwrap_or("viewer");
        // Map fixture role names to auth_role enum
        let db_role = match role {
            "admin" => "admin",
            "operator" => "operator",
            "viewer" | _ => "viewer",
        };

        sqlx::query(
            r#"
            INSERT INTO user_role_assignments (user_id, role)
            VALUES ($1, $2::auth_role)
            ON CONFLICT (user_id, role) DO NOTHING
            "#,
        )
        .bind(user_id)
        .bind(db_role)
        .execute(pool)
        .await?;

        user_ids.push((user.email.clone(), user_id));
    }

    Ok(user_ids)
}

// ---------------------------------------------------------------------------
// Seed: systems
// ---------------------------------------------------------------------------

async fn seed_systems(
    pool: &PgPool,
    systems: &[FixtureSystem],
    flake_ids: &FlakeIdMap,
    _policy_ids: &[String],
) -> Result<SystemIdMap> {
    if systems.is_empty() {
        return Ok(SystemIdMap::default());
    }

    tracing::info!("Seeding {} systems", systems.len());

    // Load environment IDs
    let env_ids = EnvIdMap::from_db(pool).await?;

    let mut system_id_map = SystemIdMap::default();

    for sys in systems {
        let environment_id = env_ids.get(&sys.environment);
        let flake_id = sys.flake.as_ref().and_then(|f| flake_ids.get(f));
        let deployment_policy = sys.deployment_policy.as_deref().unwrap_or("manual");

        // Validate policy
        let valid_policy = match deployment_policy {
            "manual" | "auto_latest" | "pinned" => deployment_policy,
            "rolling" | "canary" | _ => "manual", // fallback to manual for unknown policies
        };

        // Systems table requires `public_key` and `derivation` (both NOT NULL)
        let public_key = format!(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFIXTURE-{}",
            sys.hostname
        );
        let derivation = sys
            .store_path
            .as_deref()
            .unwrap_or("/nix/store/00000000000000000000000000000000-nixos-system-fixture");

        let system_uuid: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO systems (hostname, environment_id, is_active, public_key, flake_id, derivation, deployment_policy)
            VALUES ($1, $2, TRUE, $3, $4, $5, $6::text)
            ON CONFLICT (hostname) DO UPDATE SET
                environment_id = EXCLUDED.environment_id,
                flake_id = EXCLUDED.flake_id,
                deployment_policy = EXCLUDED.deployment_policy
            RETURNING id
            "#,
        )
        .bind(&sys.hostname)
        .bind(environment_id)
        .bind(&public_key)
        .bind(flake_id)
        .bind(derivation)
        .bind(valid_policy)
        .fetch_one(pool)
        .await
        .with_context(|| format!("Failed to seed system '{}'", sys.hostname))?;

        system_id_map.insert(sys.id.clone(), system_uuid);
    }

    Ok(system_id_map)
}

// ---------------------------------------------------------------------------
// Seed: system_states + agent_heartbeats
// ---------------------------------------------------------------------------

async fn seed_system_states(
    pool: &PgPool,
    systems: &[FixtureSystem],
    _system_ids: &SystemIdMap,
) -> Result<()> {
    if systems.is_empty() {
        return Ok(());
    }

    tracing::info!("Seeding system_states for {} systems", systems.len());
    let mut hb_count = 0usize;

    for sys in systems {
        let store_path = sys
            .store_path
            .as_deref()
            .unwrap_or("/nix/store/00000000000000000000000000000000-nixos-system-fixture");
        let kernel = sys.kernel.as_deref().unwrap_or("linux-6.1.115");
        let nixos_version = sys.nixos_version.as_deref().unwrap_or("24.05");
        let cpu_brand = sys.cpu.as_deref().unwrap_or("Unknown CPU");
        let memory_gb = sys.mem_gb;
        let primary_ip = sys.ipv4.as_deref().or(sys.ipv6.as_deref());

        // Parse uptime string like "32d 22h" into seconds
        let uptime_secs = parse_uptime(sys.uptime.as_deref().unwrap_or("0s"));

        // Parse "4m ago", "2h ago" style relative timestamps into absolute timestamps
        let hb_timestamp = parse_relative_time(sys.last_heartbeat.as_deref().unwrap_or("now"))
            .unwrap_or_else(|| chrono::Utc::now());

        let state_id: i32 = sqlx::query_scalar(
            r#"
            INSERT INTO system_states
                (hostname, store_path, change_reason, os, kernel, memory_gb, uptime_secs,
                 cpu_brand, cpu_cores, primary_ip_address, nixos_version,
                 timestamp, generation)
            VALUES ($1, $2, 'state_delta', 'nixos', $3, $4, $5,
                    $6, 4, $7, $8,
                    $9, $10)
            RETURNING id
            "#,
        )
        .bind(&sys.hostname)
        .bind(store_path)
        .bind(kernel)
        .bind(memory_gb)
        .bind(uptime_secs)
        .bind(cpu_brand)
        .bind(primary_ip)
        .bind(nixos_version)
        .bind(hb_timestamp)
        .bind(sys.generation)
        .fetch_one(pool)
        .await
        .with_context(|| format!("Failed to seed system_state for '{}'", sys.hostname))?;

        // Also create an agent_heartbeat so the view_system_list health calculation works
        sqlx::query(
            r#"
            INSERT INTO agent_heartbeats (system_state_id, timestamp)
            VALUES ($1, $2)
            "#,
        )
        .bind(state_id)
        .bind(hb_timestamp)
        .execute(pool)
        .await?;
        hb_count += 1;
    }

    tracing::info!(
        "Seeded {} system_states and {} heartbeats",
        systems.len(),
        hb_count
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Seed: pending_system_deployments + system_events
// ---------------------------------------------------------------------------

async fn seed_system_events_and_pending_deployments(
    pool: &PgPool,
    systems: &[FixtureSystem],
    system_ids: &SystemIdMap,
) -> Result<()> {
    let mut pending_count = 0usize;
    let mut event_count = 0usize;

    for sys in systems {
        let Some(system_id) = system_ids.get(&sys.id) else {
            continue;
        };

        let mut pending_deployment_id: Option<Uuid> = None;
        if let Some(pending) = sys.pending_deployment.as_ref() {
            let issued_at = parse_relative_time(pending.issued_at.as_deref().unwrap_or("now"))
                .unwrap_or_else(chrono::Utc::now);
            let delivered_at = pending
                .delivered_at
                .as_deref()
                .and_then(parse_relative_time);
            let applying_at = pending.applying_at.as_deref().and_then(parse_relative_time);
            let completed_at = pending
                .completed_at
                .as_deref()
                .and_then(parse_relative_time);
            let status = pending.status.as_deref().unwrap_or("pending");
            let source = pending.source.as_deref().unwrap_or("fixture");
            let metadata = serde_json::json!({
                "target_generation": pending.target_generation,
                "target_commit": pending.target_commit,
                "kind": pending.kind.as_deref().unwrap_or("deployment"),
                "fixture": true
            });

            let deployment_id: Uuid = sqlx::query_scalar(
                r#"
                INSERT INTO pending_system_deployments
                    (system_id, target_store_path, status, source, issued_at, expires_at,
                     completed_at, delivered_at, applying_at, metadata)
                VALUES ($1, $2, $3, $4, $5, $5 + interval '2 hours', $6, $7, $8, $9)
                RETURNING id
                "#,
            )
            .bind(system_id)
            .bind(&pending.target_store_path)
            .bind(status)
            .bind(source)
            .bind(issued_at)
            .bind(completed_at)
            .bind(delivered_at)
            .bind(applying_at)
            .bind(metadata)
            .fetch_one(pool)
            .await
            .with_context(|| format!("Failed to seed pending deployment for '{}'", sys.hostname))?;

            pending_deployment_id = Some(deployment_id);
            pending_count += 1;
        }

        for (index, event) in sys.events.as_deref().unwrap_or(&[]).iter().enumerate() {
            let event_type = event
                .event_type
                .as_deref()
                .unwrap_or("cf_deployment_succeeded");
            let occurred_at = parse_relative_time(event.at.as_deref().unwrap_or("now"))
                .unwrap_or_else(chrono::Utc::now);
            let source = event.source.as_deref().unwrap_or("fixture");
            let metadata = serde_json::json!({
                "title": event.title,
                "outcome": event.outcome,
                "commit_hash": event.commit_hash,
                "fixture_color": event.color,
                "fixture": true
            });
            let dedupe_key = format!("fixture:{}:{}:{}", sys.id, event_type, index);

            sqlx::query(
                r#"
                INSERT INTO system_events
                    (system_id, event_type, event_rank, dedupe_key, occurred_at, observed_at,
                     new_generation, new_store_path, deployment_id, desired_target_id,
                     source, actor, metadata)
                VALUES ($1, $2, $3, $4, $5, $5, $6, $7, $8, $8, $9, $10, $11)
                ON CONFLICT (system_id, event_type, dedupe_key) DO NOTHING
                "#,
            )
            .bind(system_id)
            .bind(event_type)
            .bind(index as i16)
            .bind(dedupe_key)
            .bind(occurred_at)
            .bind(event.generation)
            .bind(event.store_path.as_deref().or(sys.store_path.as_deref()))
            .bind(pending_deployment_id)
            .bind(source)
            .bind(event.actor.as_deref().unwrap_or("fixture"))
            .bind(metadata)
            .execute(pool)
            .await
            .with_context(|| format!("Failed to seed system event for '{}'", sys.hostname))?;
            event_count += 1;
        }
    }

    tracing::info!(
        "Seeded {} pending deployments and {} system events",
        pending_count,
        event_count
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Seed: CVEs
// ---------------------------------------------------------------------------

async fn seed_cves(
    pool: &PgPool,
    cves: &FixtureCves,
    _systems: &[FixtureSystem],
    _system_ids: &SystemIdMap,
) -> Result<()> {
    if cves.list.is_empty() {
        return Ok(());
    }

    tracing::info!("Seeding {} CVEs", cves.list.len());

    for cve in &cves.list {
        let cvss_score = cve.cvss.map(|v| (v * 10.0).round() / 10.0); // round to 1 decimal
        let exploited = cve.exploited.unwrap_or(false);

        sqlx::query(
            r#"
            INSERT INTO cves (id, cvss_v3_score, description, exploited)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (id) DO UPDATE SET
                cvss_v3_score = EXCLUDED.cvss_v3_score,
                description = EXCLUDED.description,
                exploited = EXCLUDED.exploited
            "#,
        )
        .bind(&cve.id)
        .bind(cvss_score)
        .bind(cve.title.as_deref().unwrap_or(""))
        .bind(exploited)
        .execute(pool)
        .await
        .with_context(|| format!("Failed to seed CVE '{}'", cve.id))?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Seed: builders and build jobs
// ---------------------------------------------------------------------------

async fn seed_builders_and_jobs(
    pool: &PgPool,
    builds: &FixtureBuilds,
    _systems: &[FixtureSystem],
    _system_ids: &SystemIdMap,
) -> Result<()> {
    if builds.workers.is_empty() && builds.active.is_empty() {
        return Ok(());
    }

    tracing::info!(
        "Seeding {} builders and {} build jobs",
        builds.workers.len(),
        builds.active.len() + builds.history.len()
    );

    let mut builder_id_map: std::collections::HashMap<String, Uuid> =
        std::collections::HashMap::new();

    // Seed builders
    for worker in &builds.workers {
        let public_key = worker
            .public_key
            .as_deref()
            .unwrap_or("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFIXTURE-BUILDER");
        let arch = worker.arch.as_deref().unwrap_or("x86_64-linux");
        let status = worker.status.as_deref().unwrap_or("active");

        // Map fixture status to allowed values
        let db_status = match status {
            "running" | "active" => "active",
            "idle" => "active",
            "offline" => "offline",
            "draining" | "drain" => "draining",
            _ => "active",
        };

        let builder_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO builders (name, public_key, status, arch, enabled, max_concurrent_jobs)
            VALUES ($1, $2, $3::text, $4::text, TRUE, 1)
            ON CONFLICT (name) DO UPDATE SET
                status = EXCLUDED.status,
                public_key = EXCLUDED.public_key
            RETURNING id
            "#,
        )
        .bind(&worker.name)
        .bind(public_key)
        .bind(db_status)
        .bind(arch)
        .fetch_one(pool)
        .await
        .with_context(|| format!("Failed to seed builder '{}'", worker.name))?;

        builder_id_map.insert(worker.id.clone(), builder_id);
    }

    // We skip seeding build_jobs because they require FK to derivations table,
    // which is complex to populate from fixture data. Build jobs will show
    // empty state which is better than broken FK errors.

    tracing::info!(
        "Seeded {} builders (build_jobs skipped - needs derivations FK)",
        builder_id_map.len()
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Seed: hardening
// ---------------------------------------------------------------------------

async fn seed_hardening(
    pool: &PgPool,
    hardening: &[FixtureHardening],
    _systems: &[FixtureSystem],
    _system_ids: &SystemIdMap,
) -> Result<()> {
    if hardening.is_empty() {
        return Ok(());
    }

    // Note: hardening_scans and service_hardening_results have FKs to
    // derivations, which we don't seed. So we skip the detailed records
    // and note that hardening data will show as empty.

    tracing::info!(
        "Hardening seeding skipped (requires derivations FK) - {} hardening items in fixture",
        hardening.len()
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Utility: parse uptime string
// ---------------------------------------------------------------------------

fn parse_uptime(s: &str) -> Option<i64> {
    let s = s.trim();
    if s == "0s" || s.is_empty() || s == "-" {
        return None;
    }

    let mut total_secs: i64 = 0;
    let mut num = String::new();

    for ch in s.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            num.push(ch);
        } else {
            let value: f64 = num.parse().unwrap_or(0.0);
            match ch {
                'd' => total_secs += (value * 86400.0) as i64,
                'h' => total_secs += (value * 3600.0) as i64,
                'm' => total_secs += (value * 60.0) as i64,
                's' => total_secs += value as i64,
                _ => {}
            }
            num.clear();
        }
    }

    if total_secs == 0 {
        None
    } else {
        Some(total_secs)
    }
}

// ---------------------------------------------------------------------------
// Utility: parse relative time like "4m ago", "2h ago"
// ---------------------------------------------------------------------------

fn parse_relative_time(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let s = s.trim();
    if s == "now" || s == "just now" || s.is_empty() {
        return Some(chrono::Utc::now());
    }

    let parts: Vec<&str> = s.splitn(2, ' ').collect();
    if parts.len() < 1 {
        return Some(chrono::Utc::now());
    }

    let num_str = parts[0].trim_end_matches(|c: char| !c.is_ascii_digit());
    let unit_part = parts[0].trim_start_matches(|c: char| c.is_ascii_digit());
    let num: i64 = num_str.parse().unwrap_or(0);

    let duration = match unit_part {
        "s" | "sec" | "secs" => chrono::Duration::seconds(num),
        "m" | "min" | "mins" => chrono::Duration::minutes(num),
        "h" | "hr" | "hrs" | "hour" | "hours" => chrono::Duration::hours(num),
        "d" | "day" | "days" => chrono::Duration::days(num),
        _ => return Some(chrono::Utc::now()),
    };

    Some(chrono::Utc::now() - duration)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the fixture JSON fully deserializes into our structs.
    /// Run manually from the repo root: `cargo test test_deserialize_fixture_json -- --include-ignored`
    #[test]
    #[ignore = "requires docs/ tree not present in Nix sandbox"]
    fn test_deserialize_fixture_json() {
        // Look for the fixture file in several common locations
        let fixture_suffix =
            std::path::Path::new("docs/design/CrystalForge/fixtures/crystal-forge.fixtures.json");
        let mut candidates = Vec::new();
        let mut ancestor = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            candidates.push(ancestor.join(fixture_suffix));
            if !ancestor.pop() {
                break;
            }
        }

        let mut content = None;
        for path in &candidates {
            if let Ok(c) = std::fs::read_to_string(path) {
                content = Some(c);
                break;
            }
        }

        let content = content
            .expect("Fixture file not found. Try running from packages/default/ or the repo root.");

        let fixture: FixtureRoot = serde_json::from_str(&content)
            .expect("Fixture JSON should deserialize into FixtureRoot");

        assert!(!fixture.environments.is_empty(), "Should have environments");
        assert!(!fixture.systems.is_empty(), "Should have systems");
        assert!(
            !fixture.builds.workers.is_empty(),
            "Should have build workers"
        );
        assert!(!fixture.policies.is_empty(), "Should have policies");
        assert!(!fixture.cves.list.is_empty(), "Should have CVEs");
        assert!(!fixture.admin.users.is_empty(), "Should have admin users");

        // Verify seed-critical identity fields. Other design-only registries
        // and optional fields are intentionally opaque or optional so the
        // canonical fixture can evolve independently.
        let first_system = &fixture.systems[0];
        assert!(
            !first_system.hostname.is_empty(),
            "System should have hostname"
        );
        assert!(
            !first_system.environment.is_empty(),
            "System should have environment"
        );
    }

    #[test]
    fn test_parse_uptime() {
        assert_eq!(
            parse_uptime("32d 22h"),
            Some((32 * 86400 + 22 * 3600) as i64)
        );
        assert_eq!(parse_uptime("0s"), None);
        assert_eq!(parse_uptime(""), None);
        assert_eq!(parse_uptime("141s"), Some(141));
        assert_eq!(parse_uptime("5d"), Some(5 * 86400));
    }

    #[test]
    fn test_parse_relative_time() {
        // Relative times should produce a timestamp in the past
        let now = chrono::Utc::now();
        let parsed = parse_relative_time("4m ago").unwrap();
        assert!(parsed < now);
        assert!(parsed > now - chrono::Duration::hours(1));

        let parsed = parse_relative_time("just now").unwrap();
        let diff = now - parsed;
        assert!(diff.num_seconds() < 5);

        let parsed = parse_relative_time("now").unwrap();
        let diff = now - parsed;
        assert!(diff.num_seconds() < 5);
    }
}
