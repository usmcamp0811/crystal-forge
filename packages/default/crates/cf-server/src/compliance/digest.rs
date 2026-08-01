//! Rust-authoritative canonical digest helpers (P1#4).
//!
//! SQL triggers set `semantic_digest = 'pending'` as a sentinel.
//! These functions compute the real SHA-256 digest and persist it, making Rust
//! the single canonical digest implementation.

use anyhow::Result;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::canonical::semantic_digest;

/// Canonical field set for a deployment policy version.
///
/// Matches the `cf-model-json-1` field list:
/// canonicalization_version, config, description, execution_phase,
/// implementation_state, name, policy_type.
pub fn policy_version_digest(
    name: &str,
    description: Option<&str>,
    policy_type: &str,
    config: &Value,
) -> String {
    let canonical = json!({
        "canonicalization_version": "cf-model-json-1",
        "config": config,
        "description": description.unwrap_or(""),
        "execution_phase": "nix-evaluation",
        "implementation_state": "native",
        "name": name,
        "policy_type": policy_type,
    });
    semantic_digest(&canonical)
}

/// Canonical field set for a compliance bundle version.
///
/// Matches the `cf-model-json-1` field list:
/// canonicalization_version, description, framework, framework_version,
/// layer, name, owner, policy_version_ids (ordered).
pub fn bundle_version_digest(
    name: &str,
    framework: &str,
    framework_version: Option<&str>,
    description: Option<&str>,
    layer: &str,
    owner: &str,
    policy_version_ids: &[Uuid],
) -> String {
    let ids: Vec<String> = policy_version_ids.iter().map(|id| id.to_string()).collect();
    let canonical = json!({
        "canonicalization_version": "cf-model-json-1",
        "description": description.unwrap_or(""),
        "framework": framework,
        "framework_version": framework_version.unwrap_or(""),
        "layer": layer,
        "name": name,
        "owner": owner,
        "policy_version_ids": ids,
    });
    semantic_digest(&canonical)
}

/// Persist the computed policy version digest after a trigger-based insert/update.
///
/// After a policy lineage INSERT/UPDATE, the sync trigger sets
/// `semantic_digest = 'pending'` on the draft version. This function computes
/// the real digest and patches it, keeping Rust as the canonical implementation.
pub async fn refresh_policy_version_digest(
    pool: &PgPool,
    policy_id: Uuid,
    name: &str,
    description: Option<&str>,
    policy_type: &str,
    config: &Value,
) -> Result<()> {
    let digest = policy_version_digest(name, description, policy_type, config);
    sqlx::query(
        r#"
        UPDATE deployment_policy_versions
        SET semantic_digest = $1,
            digest_algorithm = 'sha-256',
            canonicalization_version = 'cf-model-json-1'
        WHERE policy_id = $2
          AND publication_state = 'draft'
          AND semantic_digest = 'pending'
        "#,
    )
    .bind(&digest)
    .bind(policy_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Persist the computed bundle version digest after a trigger-based insert/update.
///
/// Reads the ordered policy version membership from `compliance_bundle_version_policies`
/// to ensure the digest reflects the exact versioned baseline, not the legacy
/// `compliance_bundle_policies` lineage table.
pub async fn refresh_bundle_version_digest(pool: &PgPool, bundle_id: Uuid) -> Result<()> {
    // Fetch current bundle metadata and its draft version id.
    #[derive(sqlx::FromRow)]
    struct BundleDigestRow {
        name: String,
        framework: String,
        framework_version: Option<String>,
        description: Option<String>,
        layer: String,
        owner: String,
        current_draft_version_id: Option<Uuid>,
    }
    let bundle_row: Option<BundleDigestRow> = sqlx::query_as(
        r#"
        SELECT
            b.name, b.framework, b.version AS framework_version,
            b.description, b.layer, b.owner,
            b.current_draft_version_id
        FROM compliance_bundles b
        WHERE b.id = $1
        "#,
    )
    .bind(bundle_id)
    .fetch_optional(pool)
    .await?;

    let Some(row) = bundle_row else {
        return Ok(());
    };
    let Some(version_id) = row.current_draft_version_id else {
        return Ok(());
    };

    // Read ordered policy version IDs from the versioned membership table.
    let policy_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT policy_version_id
        FROM compliance_bundle_version_policies
        WHERE bundle_version_id = $1
        ORDER BY policy_order ASC
        "#,
    )
    .bind(version_id)
    .fetch_all(pool)
    .await?;

    let digest = bundle_version_digest(
        &row.name,
        &row.framework,
        row.framework_version.as_deref(),
        row.description.as_deref(),
        &row.layer,
        &row.owner,
        &policy_ids,
    );

    sqlx::query(
        r#"
        UPDATE compliance_bundle_versions
        SET semantic_digest = $1,
            digest_algorithm = 'sha-256',
            canonicalization_version = 'cf-model-json-1'
        WHERE id = $2
          AND publication_state = 'draft'
        "#,
    )
    .bind(&digest)
    .bind(version_id)
    .execute(pool)
    .await?;

    // Also refresh assignment effective_set_digest for all draft assignments.
    sqlx::query(
        r#"
        UPDATE compliance_bundle_assignments
        SET effective_set_digest = $1
        WHERE bundle_version_id = $2
        "#,
    )
    .bind(&digest)
    .bind(version_id)
    .execute(pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_digest_is_stable_across_calls() {
        let config = json!({"expression": "cfg.config.networking.firewall.enable"});
        let a = policy_version_digest("firewall", Some("Firewall check"), "custom_check", &config);
        let b = policy_version_digest("firewall", Some("Firewall check"), "custom_check", &config);
        assert_eq!(a, b, "digest must be deterministic");
    }

    #[test]
    fn policy_digest_changes_when_name_changes() {
        let config = json!({"expression": "true"});
        let a = policy_version_digest("pol-a", None, "custom_check", &config);
        let b = policy_version_digest("pol-b", None, "custom_check", &config);
        assert_ne!(a, b);
    }

    #[test]
    fn policy_digest_changes_when_config_changes() {
        let a = policy_version_digest("p", None, "custom_check", &json!({"x": 1}));
        let b = policy_version_digest("p", None, "custom_check", &json!({"x": 2}));
        assert_ne!(a, b);
    }

    #[test]
    fn bundle_digest_is_stable() {
        let ids = vec![
            Uuid::parse_str("11111111-0000-0000-0000-000000000001").unwrap(),
            Uuid::parse_str("11111111-0000-0000-0000-000000000002").unwrap(),
        ];
        let a = bundle_version_digest("B", "STIG", Some("V1R1"), Some("Desc"), "os", "Me", &ids);
        let b = bundle_version_digest("B", "STIG", Some("V1R1"), Some("Desc"), "os", "Me", &ids);
        assert_eq!(a, b);
    }

    #[test]
    fn bundle_digest_changes_on_policy_order_change() {
        let id1 = Uuid::parse_str("11111111-0000-0000-0000-000000000001").unwrap();
        let id2 = Uuid::parse_str("11111111-0000-0000-0000-000000000002").unwrap();
        let a = bundle_version_digest("B", "STIG", None, None, "os", "Me", &[id1, id2]);
        let b = bundle_version_digest("B", "STIG", None, None, "os", "Me", &[id2, id1]);
        assert_ne!(a, b, "order must matter");
    }

    #[test]
    fn bundle_digest_changes_when_framework_version_changes() {
        let a = bundle_version_digest("B", "STIG", Some("V1R1"), None, "os", "Me", &[]);
        let b = bundle_version_digest("B", "STIG", Some("V1R2"), None, "os", "Me", &[]);
        assert_ne!(a, b);
    }

    #[test]
    fn bundle_digest_changes_when_description_changes() {
        let a = bundle_version_digest("B", "STIG", None, Some("Desc A"), "os", "Me", &[]);
        let b = bundle_version_digest("B", "STIG", None, Some("Desc B"), "os", "Me", &[]);
        assert_ne!(a, b);
    }
}
