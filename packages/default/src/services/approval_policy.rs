// Approval policy evaluation service
// Tracks and verifies operator approvals for deployment policies

use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::deployment_policies::ApprovalConfig;

/// Result of approval policy evaluation
#[derive(Debug, Clone)]
pub struct ApprovalResult {
    pub deployment_allowed: bool,
    pub approvals_received: usize,
    pub approvals_required: usize,
    pub reason: Option<String>,
}

/// Deployment context types for approvals
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeploymentContext {
    Commit,
    Derivation,
    SystemDeployment,
}

impl DeploymentContext {
    pub fn as_str(&self) -> &'static str {
        match self {
            DeploymentContext::Commit => "commit",
            DeploymentContext::Derivation => "derivation",
            DeploymentContext::SystemDeployment => "system_deployment",
        }
    }
}

/// Check if deployment has required approvals
pub async fn check_approvals(
    pool: &PgPool,
    context: DeploymentContext,
    context_id: &str,
    policy_id: Uuid,
    config: &ApprovalConfig,
) -> Result<ApprovalResult, sqlx::Error> {
    // Get non-expired approvals for this deployment
    let mut approvals = get_approvals(pool, context, context_id, policy_id).await?;
    
    // Filter out expired approvals
    if let Some(expires_hours) = config.expires_after_hours {
        let cutoff = Utc::now() - Duration::hours(expires_hours as i64);
        approvals.retain(|approval| approval.approved_at > cutoff);
    }
    
    let approval_count = approvals.len();
    let required_count = config.count as usize;
    
    // Check if distinct approvers requirement is met
    if config.distinct {
        let unique_approvers: std::collections::HashSet<_> = 
            approvals.iter().map(|a| a.approved_by).collect();
        if unique_approvers.len() < approval_count {
            return Ok(ApprovalResult {
                deployment_allowed: false,
                approvals_received: unique_approvers.len(),
                approvals_required: required_count,
                reason: Some("Duplicate approvals from same user detected".to_string()),
            });
        }
    }
    
    // TODO: Verify approvers have required role (requires user role lookup)
    // For now, we trust that the approval was created by someone with the right role
    
    if approval_count >= required_count {
        Ok(ApprovalResult {
            deployment_allowed: true,
            approvals_received: approval_count,
            approvals_required: required_count,
            reason: None,
        })
    } else {
        Ok(ApprovalResult {
            deployment_allowed: false,
            approvals_received: approval_count,
            approvals_required: required_count,
            reason: Some(format!(
                "Only {}/{} approvals received",
                approval_count, required_count
            )),
        })
    }
}

/// Submit an approval for a deployment
pub async fn submit_approval(
    pool: &PgPool,
    context: DeploymentContext,
    context_id: &str,
    policy_id: Uuid,
    approved_by: Uuid,
    comment: Option<String>,
    expires_after_hours: Option<u32>,
) -> Result<Uuid, sqlx::Error> {
    let expires_at = expires_after_hours.map(|hours| {
        Utc::now() + Duration::hours(hours as i64)
    });
    
    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO deployment_approvals (
            deployment_context_type,
            deployment_context_id,
            policy_id,
            approved_by,
            comment,
            expires_at
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (deployment_context_type, deployment_context_id, policy_id, approved_by)
        DO UPDATE SET
            approved_at = now(),
            comment = EXCLUDED.comment,
            expires_at = EXCLUDED.expires_at
        RETURNING id
        "#,
    )
    .bind(context.as_str())
    .bind(context_id)
    .bind(policy_id)
    .bind(approved_by)
    .bind(comment)
    .bind(expires_at)
    .fetch_one(pool)
    .await?;
    
    Ok(id)
}

/// Approval record from database
#[derive(Debug, Clone)]
struct ApprovalRecord {
    approved_by: Uuid,
    approved_at: DateTime<Utc>,
}

/// Get approvals for a deployment
async fn get_approvals(
    pool: &PgPool,
    context: DeploymentContext,
    context_id: &str,
    policy_id: Uuid,
) -> Result<Vec<ApprovalRecord>, sqlx::Error> {
    let approvals = sqlx::query_as::<_, (Uuid, DateTime<Utc>)>(
        r#"
        SELECT approved_by, approved_at
        FROM deployment_approvals
        WHERE deployment_context_type = $1
          AND deployment_context_id = $2
          AND policy_id = $3
          AND (expires_at IS NULL OR expires_at > now())
        ORDER BY approved_at DESC
        "#,
    )
    .bind(context.as_str())
    .bind(context_id)
    .bind(policy_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(approved_by, approved_at)| ApprovalRecord {
        approved_by,
        approved_at,
    })
    .collect();
    
    Ok(approvals)
}
