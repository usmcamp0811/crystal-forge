//! Canonical digest helpers for compliance framework versions.
//!
//! Follows the same `'pending'` sentinel + Rust-authoritative-compute pattern
//! established by [`super::digest`] for policy and bundle versions.
//!
//! # Canonical field set for a framework version
//!
//! ```text
//! canonicalization_version, canonical_release_key, canonical_source_key,
//! publisher, title, version
//! ```
//!
//! Note: `published_at` and `source_artifact_id` are provenance metadata and
//! are intentionally excluded from the semantic digest. The digest covers only
//! the framework content identity, not where or when it was ingested.

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::BTreeSet;
use uuid::Uuid;

use super::canonical::semantic_digest;
use super::interchange::InterchangeLimits;
use super::requirement_model::RequirementVersionCanonical;
use super::requirement_model::write_requirement_version_digest;
use super::xccdf::disa_stig_adapter::{
    canonical_for_group, canonical_for_rule, canonical_framework_requirements_for_framework,
    canonical_key_for_rule, hierarchy_edges_for_framework, identify_framework, is_disa_stig,
};
use super::xccdf::package::process_xccdf_bytes;

pub const FRAMEWORK_CANONICALIZATION_VERSION: &str = "cf-model-json-4";

pub fn requirement_semantic_digests(requirements: &[RequirementVersionCanonical]) -> Vec<String> {
    let mut digests: Vec<String> = requirements
        .iter()
        .map(RequirementVersionCanonical::compute_digest)
        .collect();
    digests.sort();
    digests.dedup();
    digests
}

// ── Canonical DTO ─────────────────────────────────────────────────────────────

/// All semantic fields for a compliance framework version digest.
#[derive(Debug, Clone)]
pub struct FrameworkVersionCanonical {
    /// The stable key for the framework lineage, e.g. `"disa-anduril-nixos-stig"`.
    pub canonical_source_key: String,
    /// The adapter-determined release identifier, e.g. `"V1R1"`.
    pub canonical_release_key: String,
    /// Human-readable version string, e.g. `"V1R1"`.
    pub version: String,
    /// Publishing organisation, e.g. `"DISA"`.
    pub publisher: Option<String>,
    /// Human-readable title, e.g. `"Anduril NixOS STIG V1R1"`.
    pub title: Option<String>,
}

impl FrameworkVersionCanonical {
    pub fn to_digest_value(&self) -> Value {
        json!({
            "canonicalization_version": FRAMEWORK_CANONICALIZATION_VERSION,
            "canonical_release_key": self.canonical_release_key,
            "canonical_source_key": self.canonical_source_key,
            "publisher": self.publisher.as_deref().unwrap_or(""),
            "title": self.title.as_deref().unwrap_or(""),
            "version": self.version,
        })
    }

    pub fn compute_digest(&self) -> String {
        self.compute_digest_with_requirement_digests([])
    }

    pub fn compute_digest_with_requirement_digests<'a, I>(&self, requirement_digests: I) -> String
    where
        I: IntoIterator<Item = &'a String>,
    {
        self.compute_digest_with_requirement_digests_and_hierarchy(requirement_digests, [])
    }

    pub fn compute_digest_with_requirement_digests_and_hierarchy<'a, I, H>(
        &self,
        requirement_digests: I,
        hierarchy_edges: H,
    ) -> String
    where
        I: IntoIterator<Item = &'a String>,
        H: IntoIterator<Item = &'a String>,
    {
        let mut value = self.to_digest_value();
        let digests: BTreeSet<&str> = requirement_digests
            .into_iter()
            .map(String::as_str)
            .collect();
        let hierarchy_edges: BTreeSet<&str> =
            hierarchy_edges.into_iter().map(String::as_str).collect();
        value["requirement_semantic_digests"] = json!(digests.into_iter().collect::<Vec<_>>());
        value["hierarchy_edges"] = json!(hierarchy_edges.into_iter().collect::<Vec<_>>());
        semantic_digest(&value)
    }
}

// ── DB writer ─────────────────────────────────────────────────────────────────

/// Write the current framework semantic digest for a framework version within `tx`.
///
/// Must be called immediately after the `INSERT` while the transaction is still
/// open.  Fails if the row is not found (which would indicate a logic error in
/// the caller).
pub async fn write_framework_version_digest(
    tx: &mut Transaction<'_, Postgres>,
    framework_version_id: Uuid,
    canonical: &FrameworkVersionCanonical,
) -> Result<()> {
    write_framework_version_digest_with_requirement_digests_and_hierarchy(
        tx,
        framework_version_id,
        canonical,
        [],
        [],
    )
    .await
}

pub async fn write_framework_version_digest_with_requirement_digests<'a, I>(
    tx: &mut Transaction<'_, Postgres>,
    framework_version_id: Uuid,
    canonical: &FrameworkVersionCanonical,
    requirement_digests: I,
) -> Result<()>
where
    I: IntoIterator<Item = &'a String>,
{
    write_framework_version_digest_with_requirement_digests_and_hierarchy(
        tx,
        framework_version_id,
        canonical,
        requirement_digests,
        [],
    )
    .await
}

pub async fn write_framework_version_digest_with_requirement_digests_and_hierarchy<'a, I, H>(
    tx: &mut Transaction<'_, Postgres>,
    framework_version_id: Uuid,
    canonical: &FrameworkVersionCanonical,
    requirement_digests: I,
    hierarchy_edges: H,
) -> Result<()>
where
    I: IntoIterator<Item = &'a String>,
    H: IntoIterator<Item = &'a String>,
{
    let digest = canonical.compute_digest_with_requirement_digests_and_hierarchy(
        requirement_digests,
        hierarchy_edges,
    );
    sqlx::query(
        "UPDATE compliance_framework_versions \
         SET semantic_digest = $1, digest_algorithm = 'sha-256', \
             canonicalization_version = $3 \
         WHERE id = $2",
    )
    .bind(&digest)
    .bind(framework_version_id)
    .bind(FRAMEWORK_CANONICALIZATION_VERSION)
    .execute(&mut **tx)
    .await
    .context("failed to write framework version semantic digest")?;
    Ok(())
}

pub async fn backfill_pending_framework_version_digests(pool: &PgPool) -> Result<()> {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM compliance_framework_versions WHERE semantic_digest = 'pending'",
    )
    .fetch_all(pool)
    .await
    .context("failed to list pending framework version digests")?;

    for id in ids {
        let mut tx = pool.begin().await?;
        let row: Option<(
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            String,
            Option<Uuid>,
        )> = sqlx::query_as(
            "SELECT f.canonical_source_key, fv.canonical_release_key,
                    fv.version, f.publisher, fv.title, fv.semantic_digest,
                    fv.source_artifact_id
             FROM compliance_framework_versions fv
             JOIN compliance_frameworks f ON f.id = fv.framework_id
             WHERE fv.id = $1
             FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((
            source_key,
            release_key,
            version,
            publisher,
            title,
            semantic_digest,
            source_artifact_id,
        )) = row
        else {
            continue;
        };
        if semantic_digest != "pending" {
            tx.commit().await?;
            continue;
        }
        let requires_stig_reconstruction = publisher
            .as_deref()
            .is_some_and(|publisher| publisher.eq_ignore_ascii_case("DISA"));
        let mut parsed_stig = None;
        if let Some(source_artifact_id) = source_artifact_id {
            let source: Option<(Vec<u8>, String)> = sqlx::query_as(
                "SELECT content, filename FROM compliance_source_artifacts WHERE id = $1",
            )
            .bind(source_artifact_id)
            .fetch_optional(&mut *tx)
            .await?;
            if let Some((content, filename)) = source {
                match process_xccdf_bytes(content, Some(filename), &InterchangeLimits::default()) {
                    Ok(package) => {
                        if is_disa_stig(&package.parsed) {
                            let source_identity = identify_framework(&package.parsed).ok_or_else(|| {
                                anyhow::anyhow!(
                                    "cannot identify legacy DISA framework source artifact for {id}"
                                )
                            })?;
                            if source_identity.canonical_source_key != source_key
                                || source_identity.canonical_release_key != release_key
                            {
                                bail!(
                                    "legacy DISA framework source artifact identity does not match framework version {id}"
                                );
                            }
                            persist_legacy_stig_topology(&mut tx, id, &package.parsed).await?;
                            parsed_stig = Some(package.parsed);
                        } else if requires_stig_reconstruction {
                            bail!(
                                "legacy DISA framework source artifact for {id} is not a DISA STIG"
                            );
                        }
                    }
                    Err(error) if requires_stig_reconstruction => {
                        bail!(
                            "cannot parse legacy DISA framework source artifact for {id}: {error:?}"
                        );
                    }
                    Err(_) => {}
                }
            }
        }
        if parsed_stig.is_none() && requires_stig_reconstruction {
            bail!("cannot finalize legacy DISA framework version {id} without its source artifact");
        }

        let requirement_rows: Vec<(
            Uuid,
            String,
            String,
            Option<String>,
            Option<String>,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            serde_json::Value,
        )> = sqlx::query_as(
            "SELECT rv.id, r.canonical_requirement_key, rv.external_id, rv.title,
                    rv.description, rv.kind, rv.severity, rv.check_text, rv.fix_text, rv.metadata
             FROM compliance_requirement_versions rv
             JOIN compliance_requirements r ON r.id = rv.requirement_id
             WHERE rv.framework_version_id = $1
             ORDER BY r.canonical_requirement_key, rv.id",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
        for (
            requirement_id,
            key,
            external_id,
            title,
            description,
            kind,
            severity,
            check_text,
            fix_text,
            metadata,
        ) in &requirement_rows
        {
            let canonical = RequirementVersionCanonical {
                canonical_requirement_key: key.clone(),
                external_id: external_id.clone(),
                title: title.clone(),
                description: description.clone(),
                kind: kind.clone(),
                severity: severity.clone(),
                check_text: check_text.clone(),
                fix_text: fix_text.clone(),
                metadata: metadata.clone(),
            };
            let pending: bool = sqlx::query_scalar(
                "SELECT semantic_digest = 'pending' FROM compliance_requirement_versions WHERE id = $1",
            )
            .bind(requirement_id)
            .fetch_one(&mut *tx)
            .await?;
            if pending {
                write_requirement_version_digest(&mut tx, *requirement_id, &canonical).await?;
            }
        }
        let (requirements, hierarchy_edges) = if let Some(parsed) = parsed_stig {
            verify_legacy_stig_topology(&mut tx, id, &parsed).await?;
            let canonical = canonical_framework_requirements_for_framework(&parsed);
            (
                requirement_semantic_digests(&canonical),
                hierarchy_edges_for_framework(&parsed),
            )
        } else {
            let requirements: Vec<String> = sqlx::query_scalar(
                "SELECT semantic_digest FROM compliance_requirement_versions
                 WHERE framework_version_id = $1 ORDER BY semantic_digest, id",
            )
            .bind(id)
            .fetch_all(&mut *tx)
            .await?;
            let hierarchy_edges: Vec<String> = sqlx::query_scalar(
                "SELECT parent_req.canonical_requirement_key || '->' || child_req.canonical_requirement_key
                 FROM compliance_requirement_versions child
                 JOIN compliance_requirements child_req ON child_req.id = child.requirement_id
                 JOIN compliance_requirement_versions parent ON parent.id = child.parent_requirement_version_id
                 JOIN compliance_requirements parent_req ON parent_req.id = parent.requirement_id
                 WHERE child.framework_version_id = $1
                 ORDER BY parent_req.canonical_requirement_key, child_req.canonical_requirement_key",
            )
            .bind(id)
            .fetch_all(&mut *tx)
            .await?;
            (requirements, hierarchy_edges)
        };
        let canonical = FrameworkVersionCanonical {
            canonical_source_key: source_key,
            canonical_release_key: release_key,
            version,
            publisher,
            title,
        };
        write_framework_version_digest_with_requirement_digests_and_hierarchy(
            &mut tx,
            id,
            &canonical,
            &requirements,
            &hierarchy_edges,
        )
        .await?;
        tx.commit().await?;
    }
    Ok(())
}

async fn persist_legacy_stig_topology(
    tx: &mut Transaction<'_, Postgres>,
    framework_version_id: Uuid,
    parsed: &super::xccdf::models::ParsedXccdf,
) -> Result<()> {
    use crate::queries::framework_requirements::{
        insert_requirement_version_pending, upsert_requirement_lineage,
    };

    let framework_id = framework_id(tx, framework_version_id).await?;
    let mut group_versions = std::collections::HashMap::new();
    for group in &parsed.groups {
        let key = format!("group:{}", group.id);
        let requirement_id = upsert_requirement_lineage(tx, framework_id, &key).await?;
        let canonical = canonical_for_group(group);
        let existing: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM compliance_requirement_versions
             WHERE requirement_id = $1 AND framework_version_id = $2",
        )
        .bind(requirement_id)
        .bind(framework_version_id)
        .fetch_optional(&mut **tx)
        .await?;
        let version_id = if let Some(version_id) = existing {
            sqlx::query(
                "UPDATE compliance_requirement_versions
                 SET external_id = $1, title = $2, description = $3, kind = $4,
                     severity = $5, check_text = $6, fix_text = $7, metadata = $8
                 WHERE id = $9 AND semantic_digest = 'pending'",
            )
            .bind(&canonical.external_id)
            .bind(&canonical.title)
            .bind(&canonical.description)
            .bind(&canonical.kind)
            .bind(&canonical.severity)
            .bind(&canonical.check_text)
            .bind(&canonical.fix_text)
            .bind(&canonical.metadata)
            .bind(version_id)
            .execute(&mut **tx)
            .await?;
            version_id
        } else {
            insert_requirement_version_pending(
                tx,
                requirement_id,
                framework_version_id,
                &canonical,
                None,
            )
            .await?
        };
        group_versions.insert(group.id.clone(), version_id);
    }
    for rule in &parsed.rules {
        let key = canonical_key_for_rule(rule);
        let parent_id = rule
            .group_id
            .as_ref()
            .and_then(|id| group_versions.get(id))
            .copied();
        let canonical = canonical_for_rule(rule, &key);
        let updated = sqlx::query(
            "UPDATE compliance_requirement_versions rv
             SET external_id = $1, title = $2, description = $3, kind = $4,
                 severity = $5, check_text = $6, fix_text = $7, metadata = $8,
                 parent_requirement_version_id = $9
             FROM compliance_requirements r
             WHERE rv.requirement_id = r.id AND rv.framework_version_id = $10
               AND r.canonical_requirement_key = $11
               AND rv.semantic_digest = 'pending'",
        )
        .bind(&canonical.external_id)
        .bind(&canonical.title)
        .bind(&canonical.description)
        .bind(&canonical.kind)
        .bind(&canonical.severity)
        .bind(&canonical.check_text)
        .bind(&canonical.fix_text)
        .bind(&canonical.metadata)
        .bind(parent_id)
        .bind(framework_version_id)
        .bind(&key)
        .execute(&mut **tx)
        .await?;
        if updated.rows_affected() == 0 {
            let requirement_id = upsert_requirement_lineage(tx, framework_id, &key).await?;
            insert_requirement_version_pending(
                tx,
                requirement_id,
                framework_version_id,
                &canonical,
                parent_id,
            )
            .await?;
        }
    }
    Ok(())
}

async fn verify_legacy_stig_topology(
    tx: &mut Transaction<'_, Postgres>,
    framework_version_id: Uuid,
    parsed: &super::xccdf::models::ParsedXccdf,
) -> Result<()> {
    let expected_nodes: BTreeSet<String> = canonical_framework_requirements_for_framework(parsed)
        .into_iter()
        .map(|canonical| canonical.canonical_requirement_key)
        .collect();
    let persisted_nodes: BTreeSet<String> = sqlx::query_scalar(
        "SELECT r.canonical_requirement_key
         FROM compliance_requirement_versions rv
         JOIN compliance_requirements r ON r.id = rv.requirement_id
         WHERE rv.framework_version_id = $1",
    )
    .bind(framework_version_id)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .collect();
    let expected_edges: BTreeSet<String> =
        hierarchy_edges_for_framework(parsed).into_iter().collect();
    let persisted_edges: BTreeSet<String> = sqlx::query_scalar(
        "SELECT parent_req.canonical_requirement_key || '->' || child_req.canonical_requirement_key
         FROM compliance_requirement_versions child
         JOIN compliance_requirements child_req ON child_req.id = child.requirement_id
         JOIN compliance_requirement_versions parent ON parent.id = child.parent_requirement_version_id
         JOIN compliance_requirements parent_req ON parent_req.id = parent.requirement_id
         WHERE child.framework_version_id = $1",
    )
    .bind(framework_version_id)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .collect();
    if persisted_nodes != expected_nodes || persisted_edges != expected_edges {
        bail!(
            "legacy DISA topology reconstruction did not match parsed source for framework version {framework_version_id}"
        );
    }
    Ok(())
}

async fn framework_id(
    tx: &mut Transaction<'_, Postgres>,
    framework_version_id: Uuid,
) -> Result<Uuid> {
    Ok(
        sqlx::query_scalar("SELECT framework_id FROM compliance_framework_versions WHERE id = $1")
            .bind(framework_version_id)
            .fetch_one(&mut **tx)
            .await?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::framework_requirements::{
        upsert_framework_lineage, upsert_requirement_lineage,
    };

    #[tokio::test]
    #[ignore = "requires a database"]
    async fn framework_digest_backfill_recanonicalizes_pending_rows() {
        let pool =
            PgPool::connect(&std::env::var("DATABASE_URL").expect("DATABASE_URL must be set"))
                .await
                .unwrap();
        backfill_pending_framework_version_digests(&pool)
            .await
            .unwrap();
        let pending: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM compliance_framework_versions
             WHERE semantic_digest = 'pending'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(pending, 0);
    }

    #[tokio::test]
    #[ignore = "requires a database"]
    async fn legacy_stig_backfill_reconstructs_topology_before_v4_finalization() {
        let pool = PgPool::connect(&std::env::var("DATABASE_URL").unwrap())
            .await
            .unwrap();
        let key = format!("disa-legacy-upgrade-{}", Uuid::new_v4());
        let xml = format!(
            r#"<Benchmark xmlns="http://checklists.nist.gov/xccdf/1.2" id="xccdf_mil.disa.stig_benchmark_Test"><title>Legacy STIG {key}</title><version>V1R1</version><Group id="V-1"><title>Authentication</title><Rule id="xccdf_mil.disa.stig_rule_SV-1r1_rule" severity="medium"><title>Rule</title><ident system="http://cyber.mil/stigs/stig">V-1</ident></Rule></Group></Benchmark>"#
        );
        let package = process_xccdf_bytes(
            xml.as_bytes().to_vec(),
            Some("legacy.xml".to_string()),
            &InterchangeLimits::default(),
        )
        .unwrap();
        assert!(is_disa_stig(&package.parsed));
        let identity = identify_framework(&package.parsed).unwrap();

        let mut tx = pool.begin().await.unwrap();
        let framework_id =
            upsert_framework_lineage(
                &mut tx,
                "Legacy STIG",
                Some("DISA"),
                &identity.canonical_source_key,
                None,
            )
            .await
            .unwrap();
        let artifact_id: Uuid = sqlx::query_scalar(
            "INSERT INTO compliance_source_artifacts
                (content, filename, media_type, sha256, parser_version)
             VALUES ($1, 'legacy.xml', 'application/xml', encode(digest($1, 'sha256'), 'hex'), 'test')
             RETURNING id",
        )
        .bind(xml.as_bytes())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        let framework_version_id: Uuid = sqlx::query_scalar(
            "INSERT INTO compliance_framework_versions
                (framework_id, version, canonical_release_key, title,
                 source_artifact_id, semantic_digest, canonicalization_version)
              VALUES ($1, $2, $3, 'Legacy STIG', $4, 'pending', 'cf-model-json-4')
              RETURNING id",
        )
        .bind(framework_id)
        .bind(&identity.version)
        .bind(&identity.canonical_release_key)
        .bind(artifact_id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        let requirement_id = upsert_requirement_lineage(&mut tx, framework_id, "V-1")
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO compliance_requirement_versions
                (requirement_id, framework_version_id, external_id, title,
                 kind, metadata, semantic_digest)
             VALUES ($1, $2, 'V-1', 'Rule', 'rule', '{}', 'pending')",
        )
        .bind(requirement_id)
        .bind(framework_version_id)
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();

        backfill_pending_framework_version_digests(&pool)
            .await
            .unwrap();

        let version: (String, String) = sqlx::query_as(
            "SELECT semantic_digest, canonicalization_version
             FROM compliance_framework_versions WHERE id = $1",
        )
        .bind(framework_version_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_ne!(version.0, "pending");
        assert_eq!(version.1, "cf-model-json-4");
        let (group_version_id, parent, external_id): (Uuid, Option<Uuid>, String) =
            sqlx::query_as(
                "SELECT group_version.id, rule_version.parent_requirement_version_id,
                        rule_version.external_id
                 FROM compliance_requirement_versions group_version
                 JOIN compliance_requirements group_req ON group_req.id = group_version.requirement_id
                 JOIN compliance_requirement_versions rule_version
                   ON rule_version.framework_version_id = group_version.framework_version_id
                 JOIN compliance_requirements rule_req ON rule_req.id = rule_version.requirement_id
                 WHERE group_version.framework_version_id = $1
                   AND group_req.canonical_requirement_key = 'group:V-1'
                   AND rule_req.canonical_requirement_key = 'V-1'",
            )
            .bind(framework_version_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(parent, Some(group_version_id));
        assert_eq!(external_id, "xccdf_mil.disa.stig_rule_SV-1r1_rule");
    }
}
