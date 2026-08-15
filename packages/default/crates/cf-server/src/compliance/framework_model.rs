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
use sqlx::{Postgres, Transaction};
use std::collections::BTreeSet;
use uuid::Uuid;

use super::canonical::semantic_digest;

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
            "canonicalization_version": "cf-model-json-1",
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
         SET semantic_digest = $1 \
         WHERE id = $2",
    )
    .bind(&digest)
    .bind(framework_version_id)
    .execute(&mut **tx)
    .await
    .context("failed to write framework version semantic digest")?;
    Ok(())
}
