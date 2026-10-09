//! Resolves exact completed publication evidence for deployment reads.
//!
//! Callers hold snapshot, POA&M, system, pending, and exact derivation locks
//! before entering this module in a SERIALIZABLE transaction. Reads do not
//! mutate publication provenance or probe the network. Destination and
//! assignment locks retain current credentials through the delivery commit.

use anyhow::Result;
use cf_protocol::{agent::RuntimeCacheConfig, cache::CacheReadAuth};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{models::cache_destination::CacheDestination, security::cache_secrets};

/// Contains only the read plane of one evidenced, currently eligible source.
#[derive(Clone, PartialEq, Eq)]
pub struct PublicationRead {
    /// Identifies the current database destination, including after a rename.
    pub destination_id: i32,
    cache_type: String,
    cache_url: String,
    cache_public_key: Option<String>,
    attic_cache_name: Option<String>,
    cache_public_keys: Vec<String>,
    read_auth: CacheReadAuth,
}

impl std::fmt::Debug for PublicationRead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PublicationRead")
            .field("destination_id", &self.destination_id)
            .field("read_configuration", &"[REDACTED]")
            .finish()
    }
}

impl PublicationRead {
    /// Converts the committed read source into agent wire settings.
    pub fn into_runtime_cache(self) -> RuntimeCacheConfig {
        RuntimeCacheConfig {
            cache_type: self.cache_type,
            cache_url: self.cache_url,
            cache_public_key: self.cache_public_key,
            attic_cache_name: self.attic_cache_name,
            cache_public_keys: self.cache_public_keys,
            read_auth: self.read_auth,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ReadDestination {
    id: i32,
    name: String,
    enabled: bool,
    cache_type: String,
    push_to: Option<String>,
    attic_cache_name: Option<String>,
    attic_public_key: Option<String>,
    niks3_public_keys: Vec<String>,
    niks3_read_auth_mode: Option<String>,
    niks3_read_client_cert: Option<String>,
    niks3_read_client_key: Option<String>,
    niks3_read_ca_cert: Option<String>,
    niks3_read_basic_username: Option<String>,
    niks3_read_basic_password: Option<String>,
}

impl ReadDestination {
    fn read(
        &self,
        confidential: bool,
        capabilities: cf_protocol::agent::AgentCapabilities,
    ) -> Option<PublicationRead> {
        // SECURITY: General Niks3 support predates authority-bound Basic. An
        // absent Basic capability must withhold both source and target delivery.
        if !self.enabled
            || (self.cache_type.eq_ignore_ascii_case("Niks3")
                && (!capabilities.supports_niks3
                    || (self.niks3_read_auth_mode.as_deref() == Some("basic")
                        && !capabilities.supports_niks3_basic_read)))
        {
            return None;
        }
        // SECURITY: Do not load or decrypt write credentials for delivery.
        // Invalid read credentials exclude only this evidenced source.
        let destination = CacheDestination {
            enabled: self.enabled,
            cache_type: self.cache_type.clone(),
            push_to: self.push_to.clone(),
            attic_cache_name: self.attic_cache_name.clone(),
            attic_public_key: self.attic_public_key.clone(),
            niks3_public_keys: self.niks3_public_keys.clone(),
            niks3_read_auth_mode: self.niks3_read_auth_mode.clone(),
            niks3_read_client_cert: self.niks3_read_client_cert.clone(),
            niks3_read_client_key: cache_secrets::decrypt_optional(
                self.niks3_read_client_key.as_deref(),
            )
            .ok()?,
            niks3_read_ca_cert: self.niks3_read_ca_cert.clone(),
            niks3_read_basic_username: self.niks3_read_basic_username.clone(),
            niks3_read_basic_password: cache_secrets::decrypt_optional(
                self.niks3_read_basic_password.as_deref(),
            )
            .ok()?,
            ..Default::default()
        };
        let (cache_url, cache_public_keys, read_auth) = destination.read_config().ok()?;
        if !confidential && !matches!(read_auth, CacheReadAuth::None) {
            return None;
        }
        Some(PublicationRead {
            destination_id: self.id,
            cache_type: self.cache_type.clone(),
            cache_url,
            cache_public_key: self.attic_public_key.clone(),
            attic_cache_name: self.attic_cache_name.clone(),
            cache_public_keys,
            read_auth,
        })
    }
}

/// Resolves a readable source for the exact authorized derivation and output.
///
/// Durable database evidence precedes unambiguous legacy references. Disabled,
/// deleted, reassigned, unreadable, or transport-incompatible sources are skipped
/// in favor of other actual completed publications. Static and unpublished
/// destinations never supply credentials. Scope precedence applies only to
/// enabled identities resolved from exact completed publication evidence:
/// assigned sources exclude globals, but an unpublished assigned destination
/// does not. Resolve legacy ambiguity before computing that group. Read-config,
/// capability, and confidentiality failures cannot downgrade an assigned group
/// to globals; other evidenced sources within the same group remain available.
/// Publication, destination, and assignment shared locks retain this evidence
/// through commit. SSI protects concurrent evidence or assignment changes.
/// Basic reads additionally require authenticated `supports_niks3_basic_read`
/// capability. General Niks3 support alone is insufficient. Every private read
/// still requires verified confidential transport before a delivery is claimed.
///
/// # Errors
/// Returns database errors, including serialization conflicts. Callers must
/// discard all instructions and retry the entire authorization on conflict.
///
/// # Examples
/// ```no_run
/// # async fn read(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>) -> anyhow::Result<()> {
/// use crystal_forge::queries::cache_publication_reads::resolve_publication_read_tx;
/// // The caller has already locked the authorized system and exact derivation.
/// let source = resolve_publication_read_tx(
///     tx, 42, "/nix/store/authorized-output", None, false, cf_protocol::agent::AgentCapabilities::default(),
/// ).await?;
/// // Do not expose source until the caller's transaction commits.
/// # Ok(()) }
/// ```
pub async fn resolve_publication_read_tx(
    tx: &mut Transaction<'_, Postgres>,
    derivation_id: i32,
    store_path: &str,
    environment_id: Option<Uuid>,
    confidential: bool,
    capabilities: cf_protocol::agent::AgentCapabilities,
) -> Result<Option<PublicationRead>> {
    let references: Vec<(Option<i32>, Option<String>, String)> = sqlx::query_as(
        "SELECT cache_destination_id, cache_destination, cache_destination_source
         FROM cache_push_jobs
         WHERE derivation_id = $1 AND store_path = $2 AND status = 'completed'
         ORDER BY CASE cache_destination_source WHEN 'database' THEN 0 ELSE 1 END, id
         FOR SHARE",
    )
    .bind(derivation_id)
    .bind(store_path)
    .fetch_all(&mut **tx)
    .await?;
    let ids: Vec<i32> = references.iter().filter_map(|r| r.0).collect();
    let names: Vec<String> = references
        .iter()
        .filter(|r| r.2 == "legacy" && r.0.is_none())
        .filter_map(|r| r.1.clone())
        .collect();
    let destinations = sqlx::query_as::<_, ReadDestination>(
        "SELECT id, name, enabled, cache_type, push_to, attic_cache_name, attic_public_key,
                niks3_public_keys, niks3_read_auth_mode, niks3_read_client_cert,
                niks3_read_client_key, niks3_read_ca_cert, niks3_read_basic_username, niks3_read_basic_password
         FROM cache_destinations
         WHERE id = ANY($1) OR name = ANY($2) OR push_to = ANY($2)
         ORDER BY id FOR SHARE",
    )
    .bind(&ids)
    .bind(&names)
    .fetch_all(&mut **tx)
    .await?;
    let destination_ids: Vec<i32> = destinations.iter().map(|d| d.id).collect();
    let assignments: Vec<(i32, Uuid)> = sqlx::query_as(
        "SELECT cache_destination_id, environment_id FROM cache_destination_environments
         WHERE cache_destination_id = ANY($1)
         ORDER BY cache_destination_id, environment_id FOR SHARE",
    )
    .bind(&destination_ids)
    .fetch_all(&mut **tx)
    .await?;
    let is_assigned = |destination: &ReadDestination| {
        environment_id.is_some_and(|env| assignments.contains(&(destination.id, env)))
    };
    let is_global =
        |destination: &ReadDestination| !assignments.iter().any(|a| a.0 == destination.id);
    let mut candidates = Vec::new();
    for (id, reference, source) in references {
        let destination = match source.as_str() {
            "database" => id.and_then(|id| destinations.iter().find(|d| d.id == id)),
            "legacy" if id.is_none() => {
                let mut matches = destinations.iter().filter(|d| {
                    reference
                        .as_deref()
                        .is_some_and(|r| d.name == r || d.push_to.as_deref() == Some(r))
                });
                let first = matches.next();
                if matches.next().is_some() {
                    None
                } else {
                    first
                }
            }
            _ => None,
        };
        let Some(destination) = destination else {
            continue;
        };
        if destination.enabled && (is_assigned(destination) || is_global(destination)) {
            candidates.push(destination);
        }
    }
    // INVARIANT: Scope precedence considers resolved publication identities,
    // before read/transport gates. Unpublished or ambiguous assigned caches
    // cannot displace a proven global; an unreadable assigned publication can.
    let has_assigned = candidates
        .iter()
        .any(|destination| is_assigned(destination));
    for destination in candidates {
        let eligible = if has_assigned {
            is_assigned(destination)
        } else {
            is_global(destination)
        };
        if eligible {
            if let Some(read) = destination.read(confidential, capabilities) {
                return Ok(Some(read));
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::composite_enforcement::{
        authorize_and_claim_desired_target, authorize_and_claim_desired_target_with_read,
    };
    use sqlx::PgPool;

    const PATH: &str = "/nix/store/publication-target";

    #[test]
    fn niks3_publication_read_plane_gates_and_redaction() {
        let mut destination = ReadDestination {
            id: 42,
            name: "private".into(),
            enabled: true,
            cache_type: "Niks3".into(),
            push_to: Some("https://read.example".into()),
            attic_cache_name: None,
            attic_public_key: None,
            niks3_public_keys: vec!["cache:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into()],
            niks3_read_auth_mode: Some("mtls".into()),
            niks3_read_client_cert: Some(crate::security::cache_secrets::TEST_CERTIFICATE.into()),
            niks3_read_client_key: Some("private-read-secret".into()),
            niks3_read_ca_cert: None,
            niks3_read_basic_username: None,
            niks3_read_basic_password: None,
        };
        let capable = cf_protocol::agent::AgentCapabilities {
            supports_niks3: true,
            ..Default::default()
        };
        assert!(destination.read(false, capable).is_none());
        assert!(destination.read(true, Default::default()).is_none());
        let read = destination.read(true, capable).unwrap();
        assert_eq!(read.destination_id, 42);
        assert!(!format!("{read:?}").contains("private-read-secret"));
        assert!(matches!(
            read.into_runtime_cache().read_auth,
            CacheReadAuth::Mtls { .. }
        ));
        destination.niks3_public_keys.clear();
        assert!(destination.read(true, capable).is_none());
    }

    async fn fixture(pool: &PgPool) -> (Uuid, Uuid, i32, Uuid) {
        let env: Uuid = sqlx::query_scalar(
            "INSERT INTO environments (name) VALUES ('publication-env') RETURNING id",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let flake: i32 = sqlx::query_scalar("INSERT INTO flakes (name, repo_url, branch) VALUES ('publication-flake', 'https://example.invalid/publication', 'main') RETURNING id").fetch_one(pool).await.unwrap();
        let commit: i32 = sqlx::query_scalar("INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) VALUES ($1, 'old-publication-commit', now() - interval '1 year') RETURNING id").bind(flake).fetch_one(pool).await.unwrap();
        let derivation: i32 = sqlx::query_scalar("INSERT INTO derivations (commit_id, derivation_name, derivation_type, derivation_path, store_path, status_id, cf_agent_enabled, policy_requirements_met) VALUES ($1, 'publication-host', 'nixos', '/nix/store/publication.drv', $2, 10, TRUE, TRUE) RETURNING id").bind(commit).bind(PATH).fetch_one(pool).await.unwrap();
        let system: Uuid = sqlx::query_scalar("INSERT INTO systems (hostname, public_key, derivation, flake_id, environment_id, desired_target, desired_target_set_at, deployment_policy) VALUES ('publication-host', 'test-public-key', '/nix/store/current', $1, $2, $3, now(), 'manual') RETURNING id").bind(flake).bind(env).bind(PATH).fetch_one(pool).await.unwrap();
        let pending: Uuid = sqlx::query_scalar("INSERT INTO pending_system_deployments (system_id, target_store_path, source, requested_commit_id, requested_derivation_id, request_action) VALUES ($1, $2, 'manual_rollback', $3, $4, 'rollback') RETURNING id").bind(system).bind(PATH).bind(commit).bind(derivation).fetch_one(pool).await.unwrap();
        (system, env, derivation, pending)
    }

    async fn cache(pool: &PgPool, name: &str, env: Option<Uuid>) -> i32 {
        let id: i32 = sqlx::query_scalar("INSERT INTO cache_destinations (name, cache_type, enabled, push_to, niks3_public_keys, niks3_read_auth_mode, niks3_write_auth_mode, niks3_auth_token, niks3_server_url) VALUES ($1, 'Niks3', TRUE, $2, ARRAY['cache:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA='], 'none', 'token', 'write-secret', 'https://write.example') RETURNING id")
            .bind(name).bind(format!("https://{name}.example")).fetch_one(pool).await.unwrap();
        if let Some(env) = env {
            sqlx::query("INSERT INTO cache_destination_environments (cache_destination_id, environment_id) VALUES ($1, $2)").bind(id).bind(env).execute(pool).await.unwrap();
        }
        id
    }

    async fn publish(pool: &PgPool, derivation: i32, id: i32, path: &str) {
        sqlx::query("INSERT INTO cache_push_jobs (derivation_id, store_path, status, cache_destination_id, cache_destination_source, cache_destination) VALUES ($1, $2, 'completed', $3, 'database', 'original-name')")
            .bind(derivation).bind(path).bind(id).execute(pool).await.unwrap();
    }

    async fn claim(
        pool: &PgPool,
        system: Uuid,
        confidential: bool,
        capable: bool,
    ) -> crate::services::composite_enforcement::TargetDeliveryAuthorization {
        authorize_and_claim_desired_target_with_read(
            pool,
            system,
            PATH,
            confidential,
            cf_protocol::agent::AgentCapabilities {
                supports_niks3: capable,
                ..Default::default()
            },
        )
        .await
        .unwrap()
    }

    async fn unchanged(pool: &PgPool, pending: Uuid) {
        let row: (String, bool, Option<String>) = sqlx::query_as("SELECT status, delivered_at IS NULL, request_action FROM pending_system_deployments WHERE id = $1").bind(pending).fetch_one(pool).await.unwrap();
        assert_eq!(row, ("pending".into(), true, Some("rollback".into())));
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified task PostgreSQL and ephemeral database creation"]
    async fn niks3_basic_publication_withholds_old_capabilities_and_insecure_delivery(
        pool: PgPool,
    ) {
        let (system, env, derivation, pending) = fixture(&pool).await;
        let id = cache(&pool, "basic-publication", Some(env)).await;
        let encrypted =
            cache_secrets::encrypt_secret("synthetic-basic-publication-password").unwrap();
        sqlx::query("UPDATE cache_destinations SET niks3_read_auth_mode='basic',niks3_read_basic_username='synthetic-user',niks3_read_basic_password=$2 WHERE id=$1").bind(id).bind(encrypted).execute(&pool).await.unwrap();
        publish(&pool, derivation, id, PATH).await;
        for (confidential, capabilities) in [
            (true, cf_protocol::agent::AgentCapabilities::default()),
            (
                true,
                cf_protocol::agent::AgentCapabilities {
                    supports_niks3: true,
                    ..Default::default()
                },
            ),
            (
                false,
                cf_protocol::agent::AgentCapabilities {
                    supports_niks3: true,
                    supports_niks3_basic_read: true,
                },
            ),
        ] {
            let denied = authorize_and_claim_desired_target_with_read(
                &pool,
                system,
                PATH,
                confidential,
                capabilities,
            )
            .await
            .unwrap();
            assert!(denied.publication_read.is_none() && denied.target.is_none());
            unchanged(&pool, pending).await;
        }
        let delivered = authorize_and_claim_desired_target_with_read(
            &pool,
            system,
            PATH,
            true,
            cf_protocol::agent::AgentCapabilities {
                supports_niks3: true,
                supports_niks3_basic_read: true,
            },
        )
        .await
        .unwrap();
        assert!(delivered.target.is_some());
        let read = delivered.publication_read.unwrap().into_runtime_cache();
        assert!(matches!(&read.read_auth, CacheReadAuth::Basic { .. }));
        assert!(!format!("{:?}", read.read_auth).contains("synthetic-user"));
        assert!(!format!("{:?}", read.read_auth).contains("synthetic-basic-publication-password"));
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified task PostgreSQL control database"]
    async fn niks3_publication_exact_identity_rename_and_secondary_source(pool: PgPool) {
        let (system, env, derivation, pending) = fixture(&pool).await;
        let published = cache(&pool, "z-published", Some(env)).await;
        publish(&pool, derivation, published, PATH).await;
        let earlier = cache(&pool, "a-new-unpublished", Some(env)).await;
        // A wrong path on the earlier cache cannot count as evidence.
        publish(&pool, derivation, earlier, "/nix/store/wrong-path").await;
        let unrelated: i32 = sqlx::query_scalar("INSERT INTO derivations (derivation_name, derivation_type, derivation_path, store_path, status_id) VALUES ('unrelated', 'nixos', '/nix/store/unrelated.drv', $1, 10) RETURNING id")
            .bind(PATH).fetch_one(&pool).await.unwrap();
        publish(&pool, unrelated, earlier, PATH).await;
        sqlx::query("UPDATE cache_destinations SET name = 'renamed' WHERE id = $1")
            .bind(published)
            .execute(&pool)
            .await
            .unwrap();
        let delivery = claim(&pool, system, false, true).await;
        assert_eq!(delivery.target.as_deref(), Some(PATH));
        assert_eq!(delivery.publication_read.unwrap().destination_id, published);
        sqlx::query("UPDATE pending_system_deployments SET delivered_at = NULL WHERE id = $1")
            .bind(pending)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE cache_destinations SET enabled = FALSE WHERE id = $1")
            .bind(published)
            .execute(&pool)
            .await
            .unwrap();
        assert!(claim(&pool, system, true, true).await.target.is_none());
        unchanged(&pool, pending).await;
        let second = cache(&pool, "second-actual", Some(env)).await;
        publish(&pool, derivation, second, PATH).await;
        assert_eq!(
            claim(&pool, system, false, true)
                .await
                .publication_read
                .unwrap()
                .destination_id,
            second
        );
        sqlx::query("UPDATE cache_destinations SET enabled = TRUE, push_to = '' WHERE id = $1")
            .bind(published)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            claim(&pool, system, false, true)
                .await
                .publication_read
                .unwrap()
                .destination_id,
            second
        );
        sqlx::query(
            "UPDATE cache_destinations SET push_to = 'https://restored.example' WHERE id = $1",
        )
        .bind(published)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE cache_destinations SET cache_type = 'Nix' WHERE id = $1")
            .bind(second)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            claim(&pool, system, false, false)
                .await
                .publication_read
                .unwrap()
                .destination_id,
            second
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified task PostgreSQL control database"]
    async fn niks3_publication_private_capability_and_current_scope_retry(pool: PgPool) {
        let (system, env, derivation, pending) = fixture(&pool).await;
        let id = cache(&pool, "private-published", Some(env)).await;
        publish(&pool, derivation, id, PATH).await;
        sqlx::query("UPDATE cache_destinations SET niks3_read_auth_mode = 'mtls', niks3_read_client_cert = $2, niks3_read_client_key = 'private-read-secret', niks3_auth_token = 'write-secret' WHERE id = $1")
            .bind(id).bind(crate::security::cache_secrets::TEST_CERTIFICATE).execute(&pool).await.unwrap();
        for (confidential, capable) in [(false, true), (true, false), (false, false)] {
            let delivery = claim(&pool, system, confidential, capable).await;
            assert!(delivery.target.is_none() && delivery.publication_read.is_none());
            unchanged(&pool, pending).await;
        }
        assert!(
            authorize_and_claim_desired_target(&pool, system, PATH)
                .await
                .unwrap()
                .target
                .is_none()
        );
        let other: Uuid =
            sqlx::query_scalar("INSERT INTO environments (name) VALUES ('other-env') RETURNING id")
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query("UPDATE systems SET environment_id = $2 WHERE id = $1")
            .bind(system)
            .bind(other)
            .execute(&pool)
            .await
            .unwrap();
        assert!(claim(&pool, system, true, true).await.target.is_none());
        unchanged(&pool, pending).await;
        sqlx::query("UPDATE systems SET environment_id = $2 WHERE id = $1")
            .bind(system)
            .bind(env)
            .execute(&pool)
            .await
            .unwrap();
        let delivery = claim(&pool, system, true, true).await;
        assert_eq!(delivery.target.as_deref(), Some(PATH));
        let read = delivery.publication_read.unwrap();
        assert!(!format!("{read:?}").contains("private-read-secret"));
        let wire = serde_json::to_string(&read.into_runtime_cache()).unwrap();
        assert!(wire.contains("private-read-secret"));
        assert!(!wire.contains("write-secret"));
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified task PostgreSQL control database"]
    async fn niks3_publication_deleted_id_legacy_and_global_evidence(pool: PgPool) {
        let (system, env, derivation, pending) = fixture(&pool).await;
        let id = cache(&pool, "original-name", Some(env)).await;
        publish(&pool, derivation, id, PATH).await;
        sqlx::query("DELETE FROM cache_destinations WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        let replacement = cache(&pool, "original-name", Some(env)).await;
        assert!(claim(&pool, system, true, true).await.target.is_none());
        unchanged(&pool, pending).await;
        let global = cache(&pool, "a-global", None).await;
        publish(&pool, derivation, global, PATH).await;
        // The replacement has no publication of its own. Its assignment cannot
        // displace the proven global or reinterpret the deleted database ID.
        assert_eq!(
            claim(&pool, system, true, true)
                .await
                .publication_read
                .unwrap()
                .destination_id,
            global
        );
        let collision: i32 = sqlx::query_scalar("INSERT INTO cache_destinations (name, cache_type, push_to, enabled) VALUES ('legacy-collision', 'Http', 'original-name', FALSE) RETURNING id")
            .fetch_one(&pool).await.unwrap();
        sqlx::query("INSERT INTO cache_push_jobs (derivation_id, store_path, status, cache_destination, cache_destination_source) VALUES ($1, $2, 'completed', 'original-name', 'legacy')")
            .bind(derivation).bind(PATH).execute(&pool).await.unwrap();
        // Ambiguous legacy evidence does not establish an assigned candidate.
        assert_eq!(
            claim(&pool, system, false, true)
                .await
                .publication_read
                .unwrap()
                .destination_id,
            global
        );
        sqlx::query("DELETE FROM cache_destinations WHERE id = $1")
            .bind(collision)
            .execute(&pool)
            .await
            .unwrap();
        // Now the actual legacy publication resolves uniquely to the assigned
        // replacement. Scope precedence precedes database/legacy priority.
        assert_eq!(
            claim(&pool, system, false, true)
                .await
                .publication_read
                .unwrap()
                .destination_id,
            replacement
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified task PostgreSQL control database"]
    async fn niks3_publication_assigned_gates_never_downgrade_to_proven_global(pool: PgPool) {
        let (system, env, derivation, pending) = fixture(&pool).await;
        let global = cache(&pool, "global-published", None).await;
        publish(&pool, derivation, global, PATH).await;
        sqlx::query("UPDATE cache_destinations SET cache_type = 'Http' WHERE id = $1")
            .bind(global)
            .execute(&pool)
            .await
            .unwrap();
        let assigned = cache(&pool, "assigned-private", Some(env)).await;
        publish(&pool, derivation, assigned, PATH).await;
        sqlx::query("UPDATE cache_destinations SET niks3_read_auth_mode = 'mtls', niks3_read_client_cert = $2, niks3_read_client_key = 'assigned-read-key' WHERE id = $1")
            .bind(assigned).bind(crate::security::cache_secrets::TEST_CERTIFICATE).execute(&pool).await.unwrap();
        for (confidential, capable) in [(false, true), (true, false)] {
            assert!(
                claim(&pool, system, confidential, capable)
                    .await
                    .target
                    .is_none()
            );
            unchanged(&pool, pending).await;
        }
        sqlx::query("UPDATE cache_destinations SET push_to = '' WHERE id = $1")
            .bind(assigned)
            .execute(&pool)
            .await
            .unwrap();
        assert!(claim(&pool, system, true, true).await.target.is_none());
        unchanged(&pool, pending).await;
        // A public compatible alternative is allowed only within the assigned
        // publication group, even when database evidence for a global is older.
        let alternative = cache(&pool, "assigned-public", Some(env)).await;
        publish(&pool, derivation, alternative, PATH).await;
        sqlx::query("UPDATE cache_destinations SET cache_type = 'Http' WHERE id = $1")
            .bind(alternative)
            .execute(&pool)
            .await
            .unwrap();
        let delivery = claim(&pool, system, false, false).await;
        assert_eq!(delivery.target.as_deref(), Some(PATH));
        assert_eq!(
            delivery.publication_read.unwrap().destination_id,
            alternative
        );
        sqlx::query("UPDATE cache_destinations SET enabled = FALSE WHERE id = ANY($1)")
            .bind(vec![assigned, alternative])
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            claim(&pool, system, false, true)
                .await
                .publication_read
                .unwrap()
                .destination_id,
            global
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified task PostgreSQL control database"]
    async fn niks3_publication_database_precedes_legacy_and_ambiguity_fails_closed(pool: PgPool) {
        let (system, env, derivation, pending) = fixture(&pool).await;
        let legacy = cache(&pool, "legacy", Some(env)).await;
        sqlx::query("INSERT INTO cache_push_jobs (derivation_id, store_path, status, cache_destination, cache_destination_source) VALUES ($1, $2, 'completed', 'legacy', 'legacy')")
            .bind(derivation).bind(PATH).execute(&pool).await.unwrap();
        let database = cache(&pool, "database", Some(env)).await;
        publish(&pool, derivation, database, PATH).await;
        assert_eq!(
            claim(&pool, system, false, true)
                .await
                .publication_read
                .unwrap()
                .destination_id,
            database
        );
        sqlx::query("UPDATE pending_system_deployments SET delivered_at = NULL WHERE id = $1")
            .bind(pending)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE cache_destinations SET enabled = FALSE WHERE id = $1")
            .bind(database)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            claim(&pool, system, false, true)
                .await
                .publication_read
                .unwrap()
                .destination_id,
            legacy
        );
        sqlx::query("UPDATE pending_system_deployments SET delivered_at = NULL WHERE id = $1")
            .bind(pending)
            .execute(&pool)
            .await
            .unwrap();
        // Disabled aliases still make uncertain legacy identity ambiguous.
        sqlx::query("INSERT INTO cache_destinations (name, cache_type, enabled, push_to) VALUES ('collision', 'Nix', FALSE, 'legacy')").execute(&pool).await.unwrap();
        assert!(claim(&pool, system, false, true).await.target.is_none());
        unchanged(&pool, pending).await;
        sqlx::query("INSERT INTO cache_push_jobs (derivation_id, store_path, status, cache_destination, cache_destination_source) VALUES ($1, $2, 'completed', 'https://static.example', 'static')")
            .bind(derivation).bind(PATH).execute(&pool).await.unwrap();
        assert!(claim(&pool, system, false, true).await.target.is_none());
        unchanged(&pool, pending).await;
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified task PostgreSQL control database"]
    async fn niks3_publication_manual_pinned_auto_latest_retained_archive(pool: PgPool) {
        let (system, env, derivation, pending) = fixture(&pool).await;
        let id = cache(&pool, "retained-published", Some(env)).await;
        publish(&pool, derivation, id, PATH).await;
        sqlx::query("UPDATE flakes SET deleted_at = now() WHERE id = (SELECT flake_id FROM systems WHERE id = $1)").bind(system).execute(&pool).await.unwrap();
        for policy in ["manual", "pinned", "auto_latest"] {
            sqlx::query("UPDATE systems SET deployment_policy = $2 WHERE id = $1")
                .bind(system)
                .bind(policy)
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("UPDATE cache_destinations SET enabled = FALSE WHERE id = $1")
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
            assert!(claim(&pool, system, false, true).await.target.is_none());
            unchanged(&pool, pending).await;
            sqlx::query("UPDATE cache_destinations SET enabled = TRUE WHERE id = $1")
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
            assert_eq!(
                claim(&pool, system, false, true).await.target.as_deref(),
                Some(PATH)
            );
            sqlx::query("UPDATE pending_system_deployments SET delivered_at = NULL WHERE id = $1")
                .bind(pending)
                .execute(&pool)
                .await
                .unwrap();
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified task PostgreSQL control database"]
    async fn niks3_publication_unknown_historical_and_bridge_remain_retryable(pool: PgPool) {
        let (system, _, _, pending) = fixture(&pool).await;
        let historical = "/nix/store/unknown-historical";
        sqlx::query("INSERT INTO system_states (hostname, change_reason, store_path) VALUES ('publication-host', 'startup', $1)")
            .bind(historical).execute(&pool).await.unwrap();
        let authorized = crate::services::composite_enforcement::authorize_and_set_system_target(
            &pool,
            system,
            historical,
            "manual_rollback_generation",
        )
        .await
        .unwrap();
        assert!(authorized.allowed());
        assert!(
            authorize_and_claim_desired_target(&pool, system, historical)
                .await
                .unwrap()
                .target
                .is_none()
        );
        let desired: Option<String> =
            sqlx::query_scalar("SELECT desired_target FROM systems WHERE id = $1")
                .bind(system)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(desired.as_deref(), Some(historical));
        sqlx::query("UPDATE systems SET desired_target = $2 WHERE id = $1")
            .bind(system)
            .bind(PATH)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO composite_legacy_desired_targets (system_id, target_store_path) VALUES ($1, $2)").bind(system).bind(PATH).execute(&pool).await.unwrap();
        assert!(claim(&pool, system, true, true).await.target.is_none());
        let markers: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM composite_legacy_desired_targets WHERE system_id = $1",
        )
        .bind(system)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(markers, 1);
        let delivered: bool = sqlx::query_scalar(
            "SELECT delivered_at IS NULL FROM pending_system_deployments WHERE id = $1",
        )
        .bind(pending)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(delivered);
    }

    async fn concurrent_change(pool: PgPool, change: &str) {
        let (system, env, derivation, pending) = fixture(&pool).await;
        let id = cache(&pool, "race-published", Some(env)).await;
        publish(&pool, derivation, id, PATH).await;
        let mut writer = pool.begin().await.unwrap();
        sqlx::query("SELECT id FROM cache_destinations WHERE id = $1 FOR UPDATE")
            .bind(id)
            .execute(&mut *writer)
            .await
            .unwrap();
        let reader_pool = pool.clone();
        let reader = tokio::spawn(async move {
            authorize_and_claim_desired_target_with_read(
                &reader_pool,
                system,
                PATH,
                true,
                cf_protocol::agent::AgentCapabilities {
                    supports_niks3: true,
                    ..Default::default()
                },
            )
            .await
        });
        // Wait for the actual destination lock attempt, not task scheduling.
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let waiting: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE datname = current_database() AND wait_event_type = 'Lock' AND query LIKE 'SELECT id, name, enabled, cache_type%')")
                    .fetch_one(&pool).await.unwrap();
                if waiting { break; }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await.unwrap();
        assert!(!reader.is_finished());
        match change {
            "rotate" => {
                sqlx::query("UPDATE cache_destinations SET push_to = 'https://rotated.example', niks3_read_auth_mode = 'mtls', niks3_read_client_cert = $2, niks3_read_client_key = 'rotated-read-key' WHERE id = $1")
                    .bind(id).bind(crate::security::cache_secrets::TEST_CERTIFICATE).execute(&mut *writer).await.unwrap();
            }
            "assign" => {
                sqlx::query(
                    "DELETE FROM cache_destination_environments WHERE cache_destination_id = $1",
                )
                .bind(id)
                .execute(&mut *writer)
                .await
                .unwrap();
                let other: Uuid = sqlx::query_scalar(
                    "INSERT INTO environments (name) VALUES ('race-other') RETURNING id",
                )
                .fetch_one(&mut *writer)
                .await
                .unwrap();
                sqlx::query("INSERT INTO cache_destination_environments (cache_destination_id, environment_id) VALUES ($1, $2)").bind(id).bind(other).execute(&mut *writer).await.unwrap();
            }
            "delete" => {
                sqlx::query("DELETE FROM cache_destinations WHERE id = $1")
                    .bind(id)
                    .execute(&mut *writer)
                    .await
                    .unwrap();
            }
            _ => panic!("unknown race"),
        }
        writer.commit().await.unwrap();
        let result = reader.await.unwrap();
        // SERIALIZABLE must not return the credentials from the stale snapshot.
        assert!(result.is_err() || result.as_ref().unwrap().target.is_none());
        unchanged(&pool, pending).await;
        let retry = claim(&pool, system, true, true).await;
        if change == "rotate" {
            let runtime = retry.publication_read.unwrap().into_runtime_cache();
            assert_eq!(runtime.cache_url, "https://rotated.example");
            assert!(
                serde_json::to_string(&runtime)
                    .unwrap()
                    .contains("rotated-read-key")
            );
        } else {
            assert!(retry.target.is_none());
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified task PostgreSQL control database"]
    async fn niks3_publication_rotation_race_before_claim_locks(pool: PgPool) {
        concurrent_change(pool, "rotate").await;
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified task PostgreSQL control database"]
    async fn niks3_publication_assignment_race_before_claim_locks(pool: PgPool) {
        concurrent_change(pool, "assign").await;
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified task PostgreSQL control database"]
    async fn niks3_publication_deletion_race_before_claim_locks(pool: PgPool) {
        concurrent_change(pool, "delete").await;
    }
}
