//! Database-bound half of exact technical matching.
//!
//! The pure `RequirementTechnicalIdentity` derivation lives in the
//! database-free `cf-compliance` crate; only the queries that need a live
//! connection stay here.

use serde_json::Value;
use uuid::Uuid;

use cf_compliance::xccdf::exact_technical_match::RequirementTechnicalIdentity;

/// DB query result for a policy version with its configuration.
#[derive(Debug, Clone)]
pub struct PolicyConfigRow {
    pub policy_id: Uuid,
    pub policy_version_id: Uuid,
    pub policy_name: String,
    pub config: Value,
}

/// Find all current-accepted policies whose config implements the requirement's technical enforcement.
///
/// This function is database-aware and performs the actual lookup.
/// It should only be called after the technical identity is known.
///
/// The returned candidates are *unordered* and must be deduplicated against
/// authoritative and inherited candidates by the caller.
pub async fn find_exact_technical_match_candidates(
    pool: &sqlx::PgPool,
    technical_identity: &RequirementTechnicalIdentity,
) -> anyhow::Result<Vec<PolicyConfigRow>> {
    // If there's no technical enforcement to match, return no candidates.
    if technical_identity.enforced_options.is_empty() {
        return Ok(vec![]);
    }

    // Fetch all current-accepted policy versions with their config.
    // We'll filter in-process since JSON matching in SQL would be complex
    // and we want to keep the query simple.
    let rows: Vec<(Uuid, Uuid, String, Value)> = sqlx::query_as(
        r#"
        SELECT DISTINCT dp.id, pv.id, pv.name, pv.config
        FROM deployment_policy_versions pv
        JOIN deployment_policies dp ON dp.id = pv.policy_id
        WHERE pv.publication_state = 'accepted'
          AND dp.current_published_version_id = pv.id
        ORDER BY pv.name
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| anyhow::anyhow!("failed to fetch policy configurations: {}", e))?;

    // Filter to those whose config matches the requirement's technical enforcement.
    let candidates = rows
        .into_iter()
        .filter_map(|(policy_id, policy_version_id, policy_name, config)| {
            if technical_identity.is_implemented_by(&config) {
                Some(PolicyConfigRow {
                    policy_id,
                    policy_version_id,
                    policy_name,
                    config,
                })
            } else {
                None
            }
        })
        .collect();

    Ok(candidates)
}

/// Result of commit-time validation for exact technical match reuse.
#[derive(Debug)]
pub enum ExactTechnicalMatchValidation {
    /// Validation succeeded; the policy still exactly matches the imported requirement.
    Valid {
        /// The selected policy version ID (reconfirmed).
        policy_version_id: Uuid,
        /// The technical identity re-derived from the authoritative imported requirement.
        technical_identity: RequirementTechnicalIdentity,
    },
    /// Validation failed with a machine-readable error code.
    Invalid { code: &'static str, message: String },
}

/// Revalidate an exact technical match at commit time.
///
/// This function must be called during import commit for any requirement
/// whose user selected a policy based on an exact technical match candidate.
/// It revalidates that:
///
/// 1. The imported rule's fix text still produces the same technical identity
/// 2. The selected policy version still exists, is accepted, and is the current published version
/// 3. The policy config still implements the requirement's enforcement
///
/// # Arguments
/// - `tx`: the open import transaction (so validation runs inside the same
///   transaction boundary as all subsequent mutation)
/// - `selected_policy_version_id`: the policy version the user selected (from MapExisting action)
/// - `authoritative_fix_text`: the authoritative fix text from the imported rule (not cached)
///
/// # Returns
/// - `Valid` if revalidation succeeds
/// - `Invalid` if any check fails (cannot be trusted for commit)
pub async fn revalidate_exact_technical_match(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    selected_policy_version_id: Uuid,
    authoritative_fix_text: &str,
) -> anyhow::Result<ExactTechnicalMatchValidation> {
    // Step 1: Re-derive technical identity from authoritative fix text.
    let technical_identity = RequirementTechnicalIdentity::from_fix_text(authoritative_fix_text);

    // If no technical enforcement was inferred, this cannot be a technical match.
    if technical_identity.enforced_options.is_empty() {
        return Ok(ExactTechnicalMatchValidation::Invalid {
            code: "IMPORT_REUSE_INELIGIBLE",
            message: "Re-parsing imported requirement produced no technical enforcement."
                .to_string(),
        });
    }

    // Step 2: Fetch the selected policy version.
    let policy_row: Option<(String, String, serde_json::Value)> = sqlx::query_as(
        r#"
        SELECT pv.publication_state, pv.name, pv.config
        FROM deployment_policy_versions pv
        WHERE pv.id = $1
        "#,
    )
    .bind(selected_policy_version_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| anyhow::anyhow!("failed to fetch selected policy version: {}", e))?;

    let (pub_state, policy_name, policy_config) = match policy_row {
        Some(row) => row,
        None => {
            return Ok(ExactTechnicalMatchValidation::Invalid {
                code: "IMPORT_REUSE_INELIGIBLE",
                message: format!(
                    "Policy version {} no longer exists.",
                    selected_policy_version_id
                ),
            });
        }
    };

    // Step 3: Verify publication state is accepted.
    if pub_state != "accepted" {
        return Ok(ExactTechnicalMatchValidation::Invalid {
            code: "IMPORT_REUSE_INELIGIBLE",
            message: format!(
                "Policy version {} is no longer accepted (state: {}). \
                 It may have been superseded or deprecated.",
                selected_policy_version_id, pub_state
            ),
        });
    }

    // Step 4: Verify this is the current published version for its policy.
    let is_current_published: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM deployment_policies dp
            WHERE dp.id = (SELECT policy_id FROM deployment_policy_versions WHERE id = $1)
              AND dp.current_published_version_id = $1
        )
        "#,
    )
    .bind(selected_policy_version_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| anyhow::anyhow!("failed to verify policy version recency: {}", e))?;

    if !is_current_published {
        return Ok(ExactTechnicalMatchValidation::Invalid {
            code: "IMPORT_REUSE_INELIGIBLE",
            message: format!(
                "Policy version {} is no longer the current published version. \
                 It may have been superseded.",
                selected_policy_version_id
            ),
        });
    }

    // Step 5: Re-run is_implemented_by() to confirm the match still holds.
    if !technical_identity.is_implemented_by(&policy_config) {
        return Ok(ExactTechnicalMatchValidation::Invalid {
            code: "IMPORT_REUSE_INELIGIBLE",
            message: format!(
                "Policy {} configuration no longer implements the requirement's \
                 technical enforcement. The policy may have been modified or the \
                 import decision may be stale.",
                policy_name
            ),
        });
    }

    Ok(ExactTechnicalMatchValidation::Valid {
        policy_version_id: selected_policy_version_id,
        technical_identity,
    })
}
