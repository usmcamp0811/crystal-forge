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

use anyhow::{Context, Result};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::BTreeSet;
use uuid::Uuid;

use super::canonical::semantic_digest;
use super::requirement_model::RequirementVersionCanonical;

pub const FRAMEWORK_CANONICALIZATION_VERSION: &str = "cf-model-json-2";

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
        let mut value = self.to_digest_value();
        let digests: BTreeSet<&str> = requirement_digests
            .into_iter()
            .map(String::as_str)
            .collect();
        value["requirement_semantic_digests"] = json!(digests.into_iter().collect::<Vec<_>>());
        semantic_digest(&value)
    }
}

// ── DB writer ─────────────────────────────────────────────────────────────────

/// Write the `cf-model-json-1` digest for a framework version within `tx`.
///
/// Must be called immediately after the `INSERT` while the transaction is still
/// open.  Fails if the row is not found (which would indicate a logic error in
/// the caller).
pub async fn write_framework_version_digest(
    tx: &mut Transaction<'_, Postgres>,
    framework_version_id: Uuid,
    canonical: &FrameworkVersionCanonical,
) -> Result<()> {
    write_framework_version_digest_with_requirement_digests(tx, framework_version_id, canonical, [])
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
    let digest = canonical.compute_digest_with_requirement_digests(requirement_digests);
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
        let (framework_id, source_key, release_key, version, publisher, title): (
            Uuid,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
        ) = sqlx::query_as(
            "SELECT fv.framework_id, f.canonical_source_key, fv.canonical_release_key,
                    fv.version, f.publisher, fv.title
             FROM compliance_framework_versions fv
             JOIN compliance_frameworks f ON f.id = fv.framework_id
             WHERE fv.id = $1
             FOR UPDATE",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        let requirements: Vec<String> = sqlx::query_scalar(
            "SELECT semantic_digest FROM compliance_requirement_versions
             WHERE framework_version_id = $1 ORDER BY semantic_digest, id",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
        let canonical = FrameworkVersionCanonical {
            canonical_source_key: source_key,
            canonical_release_key: release_key,
            version,
            publisher,
            title,
        };
        write_framework_version_digest_with_requirement_digests(
            &mut tx,
            id,
            &canonical,
            &requirements,
        )
        .await?;
        let _ = framework_id;
        tx.commit().await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
