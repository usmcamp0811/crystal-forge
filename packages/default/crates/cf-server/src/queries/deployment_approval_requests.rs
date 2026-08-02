//! Database queries for deployment approval requests, decisions, and authorizations.

use crate::models::deployment_approval_requests::{
    ApprovalRequestDetail, ApprovalSummary, CreateApprovalRequest, DeploymentApprovalDecision,
    DeploymentApprovalRequest, DeploymentAuthorization, EnvironmentApprovalSummary,
    SystemApprovalSummary,
};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Approval Request CRUD
// ---------------------------------------------------------------------------

/// Create a new approval request or return an existing one with the same fingerprint.
pub async fn create_or_reuse_approval_request(
    tx: &mut Transaction<'_, Postgres>,
    params: &CreateApprovalRequest,
) -> Result<DeploymentApprovalRequest> {
    // Check for existing active request with same fingerprint
    let existing = sqlx::query_as::<_, DeploymentApprovalRequest>(
        r#"
        SELECT * FROM deployment_approval_requests
        WHERE request_fingerprint = $1 AND status = 'pending'
        FOR UPDATE
        "#,
    )
    .bind(&params.request_fingerprint)
    .fetch_optional(&mut **tx)
    .await
    .context("check existing approval request")?;

    if let Some(existing) = existing {
        return Ok(existing);
    }

    // Supersede any incompatible pending requests for the same system
    sqlx::query(
        r#"
        UPDATE deployment_approval_requests
        SET status = 'superseded',
            superseded_by_id = NULL,
            updated_at = NOW()
        WHERE system_id = $1
          AND status = 'pending'
          AND request_fingerprint != $2
        "#,
    )
    .bind(params.system_id)
    .bind(&params.request_fingerprint)
    .execute(&mut **tx)
    .await
    .context("supersede old approval requests")?;

    let request = sqlx::query_as::<_, DeploymentApprovalRequest>(
        r#"
        INSERT INTO deployment_approval_requests (
            system_id, environment_id, target_store_path, target_derivation_path,
            target_commit_id, target_commit_hash, flake_id,
            deployment_policy_id, deployment_policy_version_id,
            requester_kind, requested_by_user_id, requested_by_automation,
            required_approvals, required_role, distinct_approvers, requester_may_approve,
            expires_at, status, request_fingerprint
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
            $13, $14, $15, $16, $17, 'pending', $18
        )
        RETURNING *
        "#,
    )
    .bind(params.system_id)
    .bind(params.environment_id)
    .bind(&params.target_store_path)
    .bind(&params.target_derivation_path)
    .bind(params.target_commit_id)
    .bind(&params.target_commit_hash)
    .bind(params.flake_id)
    .bind(params.deployment_policy_id)
    .bind(params.deployment_policy_version_id)
    .bind(params.requester_kind.as_str())
    .bind(params.requested_by_user_id)
    .bind(&params.requested_by_automation)
    .bind(params.required_approvals)
    .bind(&params.required_role)
    .bind(params.distinct_approvers)
    .bind(params.requester_may_approve)
    .bind(params.expires_at)
    .bind(&params.request_fingerprint)
    .fetch_one(&mut **tx)
    .await
    .context("insert approval request")?;

    Ok(request)
}

/// Get one approval request by ID.
pub async fn get_approval_request(
    pool: &PgPool,
    request_id: Uuid,
) -> Result<Option<DeploymentApprovalRequest>> {
    sqlx::query_as::<_, DeploymentApprovalRequest>(
        "SELECT * FROM deployment_approval_requests WHERE id = $1",
    )
    .bind(request_id)
    .fetch_optional(pool)
    .await
    .context("get approval request")
}

/// Get approval request with row lock for updates.
pub async fn get_approval_request_for_update(
    tx: &mut Transaction<'_, Postgres>,
    request_id: Uuid,
) -> Result<Option<DeploymentApprovalRequest>> {
    sqlx::query_as::<_, DeploymentApprovalRequest>(
        "SELECT * FROM deployment_approval_requests WHERE id = $1 FOR UPDATE",
    )
    .bind(request_id)
    .fetch_optional(&mut **tx)
    .await
    .context("get approval request for update")
}

/// Get approval request detail with decisions and progress.
pub async fn get_approval_request_detail(
    pool: &PgPool,
    request_id: Uuid,
    user_id: Uuid,
    user_role: &str,
) -> Result<Option<ApprovalRequestDetail>> {
    let request = get_approval_request(pool, request_id).await?;
    let request = match request {
        Some(r) => r,
        None => return Ok(None),
    };

    let decisions = list_decisions_for_request(pool, request_id).await?;
    let current_approval_count = decisions
        .iter()
        .filter(|d| d.decision == "approve")
        .count() as i64;

    let mut allowed_actions = Vec::new();

    if request.status == "pending" {
        // Check if user can approve
        let already_decided = decisions.iter().any(|d| d.actor_user_id == user_id);
        let is_requester = request.requested_by_user_id == Some(user_id);
        let role_ok = request
            .required_role
            .as_ref()
            .is_none_or(|r| r == user_role || user_role == "admin");

        if !already_decided && role_ok && (!is_requester || request.requester_may_approve) {
            allowed_actions.push("approve".to_string());
            allowed_actions.push("reject".to_string());
        }

        // Check if user can cancel
        if is_requester || user_role == "admin" {
            allowed_actions.push("cancel".to_string());
        }
    }

    Ok(Some(ApprovalRequestDetail {
        request,
        current_approval_count,
        decisions,
        allowed_actions,
    }))
}

/// List approval requests with optional filters.
pub async fn list_approval_requests(
    pool: &PgPool,
    status: Option<&str>,
    system_id: Option<Uuid>,
    environment_id: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<DeploymentApprovalRequest>> {
    // Build query dynamically based on filters
    let mut sql = String::from("SELECT * FROM deployment_approval_requests WHERE 1=1");
    let mut bind_idx = 1u32;

    if status.is_some() {
        sql.push_str(&format!(" AND status = ${bind_idx}"));
        bind_idx += 1;
    }
    if system_id.is_some() {
        sql.push_str(&format!(" AND system_id = ${bind_idx}"));
        bind_idx += 1;
    }
    if environment_id.is_some() {
        sql.push_str(&format!(" AND environment_id = ${bind_idx}"));
        bind_idx += 1;
    }

    // Pending first, then oldest first within pending
    sql.push_str(
        " ORDER BY CASE WHEN status = 'pending' THEN 0 ELSE 1 END, requested_at ASC",
    );
    sql.push_str(&format!(" LIMIT ${bind_idx}"));
    bind_idx += 1;
    sql.push_str(&format!(" OFFSET ${bind_idx}"));

    let mut query = sqlx::query_as::<_, DeploymentApprovalRequest>(&sql);
    if let Some(s) = status {
        query = query.bind(s);
    }
    if let Some(sid) = system_id {
        query = query.bind(sid);
    }
    if let Some(eid) = environment_id {
        query = query.bind(eid);
    }
    query = query.bind(limit).bind(offset);

    query
        .fetch_all(pool)
        .await
        .context("list approval requests")
}

// ---------------------------------------------------------------------------
// Approval Decisions
// ---------------------------------------------------------------------------

/// Record an approval decision and update request status.
pub async fn record_approval_decision(
    tx: &mut Transaction<'_, Postgres>,
    request_id: Uuid,
    actor_user_id: Uuid,
    decision: &str,
    note: Option<&str>,
    actor_role: Option<&str>,
) -> Result<(DeploymentApprovalDecision, DeploymentApprovalRequest)> {
    let request = get_approval_request_for_update(tx, request_id)
        .await?
        .context("approval request not found")?;

    // Validate request is pending
    if request.status != "pending" {
        anyhow::bail!("approval_request_not_pending");
    }

    // Check expiration
    if let Some(expires_at) = request.expires_at {
        if Utc::now() >= expires_at {
            // Expire the request
            expire_request(tx, request_id).await?;
            anyhow::bail!("approval_request_expired");
        }
    }

    // Check role requirement
    if let Some(ref required_role) = request.required_role {
        if let Some(role) = actor_role {
            if role != required_role && role != "admin" {
                anyhow::bail!("approval_role_required");
            }
        } else {
            anyhow::bail!("approval_role_required");
        }
    }

    // Check requester self-approval
    if !request.requester_may_approve && request.requested_by_user_id == Some(actor_user_id) {
        anyhow::bail!("approval_requester_not_allowed");
    }

    let status_before = request.status.clone();
    let new_status = if decision == "reject" {
        "rejected".to_string()
    } else {
        // Count existing approvals
        let existing_count = count_approvals_for_request(tx, request_id).await?;
        if existing_count + 1 >= request.required_approvals as i64 {
            "approved".to_string()
        } else {
            "pending".to_string()
        }
    };

    // Insert decision (idempotent: ON CONFLICT returns existing)
    let decision_row = sqlx::query_as::<_, DeploymentApprovalDecision>(
        r#"
        INSERT INTO deployment_approval_decisions (
            request_id, actor_user_id, decision, note,
            actor_role_snapshot, request_fingerprint,
            status_before, status_after
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (request_id, actor_user_id) DO UPDATE
            SET request_id = deployment_approval_decisions.request_id
        RETURNING *
        "#,
    )
    .bind(request_id)
    .bind(actor_user_id)
    .bind(decision)
    .bind(note)
    .bind(actor_role)
    .bind(&request.request_fingerprint)
    .bind(&status_before)
    .bind(&new_status)
    .fetch_one(&mut **tx)
    .await
    .context("insert approval decision")?;

    // Check if this was an idempotent replay (existing decision)
    if decision_row.status_before != status_before {
        // Idempotent: return existing decision without changing request
        let updated_request = get_approval_request_for_update(tx, request_id)
            .await?
            .context("approval request not found after idempotent decision")?;
        return Ok((decision_row, updated_request));
    }

    // Update request status if changed
    if new_status != "pending" {
        let authorization_id = if new_status == "approved" {
            Some(
                create_authorization_from_approval(tx, &request)
                    .await?
                    .id,
            )
        } else {
            None
        };

        sqlx::query(
            r#"
            UPDATE deployment_approval_requests
            SET status = $1,
                deployment_authorization_id = $2,
                decided_at = NOW(),
                updated_at = NOW()
            WHERE id = $3
            "#,
        )
        .bind(&new_status)
        .bind(authorization_id)
        .bind(request_id)
        .execute(&mut **tx)
        .await
        .context("update approval request status")?;
    }

    let updated_request = get_approval_request_for_update(tx, request_id)
        .await?
        .context("approval request not found after decision")?;

    Ok((decision_row, updated_request))
}

/// Count approve decisions for a request.
async fn count_approvals_for_request(
    tx: &mut Transaction<'_, Postgres>,
    request_id: Uuid,
) -> Result<i64> {
    let row = sqlx::query("SELECT COUNT(*) as count FROM deployment_approval_decisions WHERE request_id = $1 AND decision = 'approve'")
        .bind(request_id)
        .fetch_one(&mut **tx)
        .await
        .context("count approvals")?;
    Ok(row.get::<i64, _>("count"))
}

/// List all decisions for a request.
pub async fn list_decisions_for_request(
    pool: &PgPool,
    request_id: Uuid,
) -> Result<Vec<DeploymentApprovalDecision>> {
    sqlx::query_as::<_, DeploymentApprovalDecision>(
        r#"
        SELECT * FROM deployment_approval_decisions
        WHERE request_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(request_id)
    .fetch_all(pool)
    .await
    .context("list decisions for request")
}

// ---------------------------------------------------------------------------
// Request Status Transitions
// ---------------------------------------------------------------------------

/// Expire a pending request.
pub async fn expire_request(
    tx: &mut Transaction<'_, Postgres>,
    request_id: Uuid,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE deployment_approval_requests
        SET status = 'expired', decided_at = NOW(), updated_at = NOW()
        WHERE id = $1 AND status = 'pending'
        "#,
    )
    .bind(request_id)
    .execute(&mut **tx)
    .await
    .context("expire approval request")?;
    Ok(())
}

/// Cancel a pending request.
pub async fn cancel_request(
    tx: &mut Transaction<'_, Postgres>,
    request_id: Uuid,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE deployment_approval_requests
        SET status = 'cancelled', decided_at = NOW(), updated_at = NOW()
        WHERE id = $1 AND status IN ('pending', 'approved')
        "#,
    )
    .bind(request_id)
    .execute(&mut **tx)
    .await
    .context("cancel approval request")?;
    Ok(())
}

/// Expire all pending requests that have passed their expiration time.
pub async fn expire_all_overdue_requests(pool: &PgPool) -> Result<Vec<Uuid>> {
    let rows = sqlx::query(
        r#"
        UPDATE deployment_approval_requests
        SET status = 'expired', decided_at = NOW(), updated_at = NOW()
        WHERE status = 'pending'
          AND expires_at IS NOT NULL
          AND expires_at <= NOW()
        RETURNING id
        "#,
    )
    .fetch_all(pool)
    .await
    .context("expire overdue approval requests")?;

    Ok(rows.iter().map(|r| r.get::<Uuid, _>("id")).collect())
}

// ---------------------------------------------------------------------------
// Authorization
// ---------------------------------------------------------------------------

/// Create a deployment authorization from an approved request.
async fn create_authorization_from_approval(
    tx: &mut Transaction<'_, Postgres>,
    request: &DeploymentApprovalRequest,
) -> Result<DeploymentAuthorization> {
    let expires_at: Option<DateTime<Utc>> = request.expires_at;

    sqlx::query_as::<_, DeploymentAuthorization>(
        r#"
        INSERT INTO deployment_authorizations (
            system_id, target_store_path, target_derivation_path,
            target_commit_id, policy_version_id,
            source_approval_request_id, authorization_source,
            issued_by_user_id, expires_at
        ) VALUES ($1, $2, $3, $4, $5, $6, 'approval', $7, $8)
        RETURNING *
        "#,
    )
    .bind(request.system_id)
    .bind(&request.target_store_path)
    .bind(&request.target_derivation_path)
    .bind(request.target_commit_id)
    .bind(request.deployment_policy_version_id)
    .bind(request.id)
    .bind(request.requested_by_user_id)
    .bind(expires_at)
    .fetch_one(&mut **tx)
    .await
    .context("create deployment authorization")
}

/// Create a deployment authorization for policy bypass (no approval needed).
pub async fn create_bypass_authorization(
    tx: &mut Transaction<'_, Postgres>,
    system_id: Uuid,
    target_store_path: &str,
    target_derivation_path: Option<&str>,
    target_commit_id: Option<Uuid>,
    policy_version_id: Option<Uuid>,
    issued_by_user_id: Option<Uuid>,
    issued_by_automation: Option<&str>,
) -> Result<DeploymentAuthorization> {
    sqlx::query_as::<_, DeploymentAuthorization>(
        r#"
        INSERT INTO deployment_authorizations (
            system_id, target_store_path, target_derivation_path,
            target_commit_id, policy_version_id,
            authorization_source, issued_by_user_id, issued_by_automation
        ) VALUES ($1, $2, $3, $4, $5, 'policy_bypass', $6, $7)
        RETURNING *
        "#,
    )
    .bind(system_id)
    .bind(target_store_path)
    .bind(target_derivation_path)
    .bind(target_commit_id)
    .bind(policy_version_id)
    .bind(issued_by_user_id)
    .bind(issued_by_automation)
    .fetch_one(&mut **tx)
    .await
    .context("create bypass authorization")
}

/// Find a valid authorization for a system and exact target.
pub async fn find_valid_authorization(
    pool: &PgPool,
    system_id: Uuid,
    target_store_path: &str,
) -> Result<Option<DeploymentAuthorization>> {
    sqlx::query_as::<_, DeploymentAuthorization>(
        r#"
        SELECT * FROM deployment_authorizations
        WHERE system_id = $1
          AND target_store_path = $2
          AND revoked_at IS NULL
          AND (expires_at IS NULL OR expires_at > NOW())
          AND consumed_at IS NULL
        ORDER BY issued_at DESC
        LIMIT 1
        "#,
    )
    .bind(system_id)
    .bind(target_store_path)
    .fetch_optional(pool)
    .await
    .context("find valid authorization")
}

/// Mark an authorization as consumed by a deployment execution.
pub async fn consume_authorization(
    tx: &mut Transaction<'_, Postgres>,
    authorization_id: Uuid,
    deployment_execution_id: Uuid,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE deployment_authorizations
        SET consumed_at = NOW(), deployment_execution_id = $2
        WHERE id = $1 AND consumed_at IS NULL
        "#,
    )
    .bind(authorization_id)
    .bind(deployment_execution_id)
    .execute(&mut **tx)
    .await
    .context("consume authorization")?;

    // Also mark the approval request as consumed
    sqlx::query(
        r#"
        UPDATE deployment_approval_requests
        SET status = 'consumed', updated_at = NOW()
        WHERE deployment_authorization_id = $1 AND status = 'approved'
        "#,
    )
    .bind(authorization_id)
    .execute(&mut **tx)
    .await
    .context("mark approval request consumed")?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Summary & Aggregate Queries
// ---------------------------------------------------------------------------

/// Get approval summary counts for the dashboard widget.
pub async fn get_approval_summary(pool: &PgPool) -> Result<ApprovalSummary> {
    let row = sqlx::query(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE status = 'pending') AS pending,
            COUNT(*) FILTER (
                WHERE status = 'pending'
                  AND requested_at < NOW() - INTERVAL '1 hour'
            ) AS waiting_more_than_one_hour,
            COUNT(*) FILTER (
                WHERE status = 'pending'
                  AND required_approvals > 1
            ) AS requires_multiple_approvers,
            COUNT(*) FILTER (
                WHERE status = 'pending'
                  AND id IN (
                      SELECT request_id FROM deployment_approval_decisions
                      WHERE decision = 'approve'
                  )
            ) AS partially_approved
        FROM deployment_approval_requests
        "#,
    )
    .fetch_one(pool)
    .await
    .context("get approval summary")?;

    Ok(ApprovalSummary {
        pending: row.get::<i64, _>("pending"),
        waiting_more_than_one_hour: row.get::<i64, _>("waiting_more_than_one_hour"),
        requires_multiple_approvers: row.get::<i64, _>("requires_multiple_approvers"),
        partially_approved: row.get::<i64, _>("partially_approved"),
    })
}

/// Get per-system approval summaries for system list views.
pub async fn get_system_approval_summaries(
    pool: &PgPool,
    system_ids: &[Uuid],
) -> Result<Vec<(Uuid, SystemApprovalSummary)>> {
    if system_ids.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r#"
        SELECT
            system_id,
            COUNT(*) AS pending_approval_count,
            MIN(requested_at) AS oldest_pending_request_at
        FROM deployment_approval_requests
        WHERE system_id = ANY($1) AND status = 'pending'
        GROUP BY system_id
        "#,
    )
    .bind(system_ids)
    .fetch_all(pool)
    .await
    .context("get system approval summaries")?;

    Ok(rows
        .iter()
        .map(|r| {
            (
                r.get::<Uuid, _>("system_id"),
                SystemApprovalSummary {
                    pending_approval_count: r.get::<i64, _>("pending_approval_count"),
                    oldest_pending_request_at: r.get("oldest_pending_request_at"),
                },
            )
        })
        .collect())
}

/// Get per-environment approval summaries.
pub async fn get_environment_approval_summaries(
    pool: &PgPool,
    environment_ids: &[Uuid],
) -> Result<Vec<(Uuid, EnvironmentApprovalSummary)>> {
    if environment_ids.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r#"
        SELECT
            environment_id,
            COUNT(*) AS pending_approval_count,
            COUNT(DISTINCT system_id) AS systems_with_pending_approval
        FROM deployment_approval_requests
        WHERE environment_id = ANY($1)
          AND status = 'pending'
          AND environment_id IS NOT NULL
        GROUP BY environment_id
        "#,
    )
    .bind(environment_ids)
    .fetch_all(pool)
    .await
    .context("get environment approval summaries")?;

    Ok(rows
        .iter()
        .map(|r| {
            (
                r.get::<Uuid, _>("environment_id"),
                EnvironmentApprovalSummary {
                    pending_approval_count: r.get::<i64, _>("pending_approval_count"),
                    systems_with_pending_approval: r
                        .get::<i64, _>("systems_with_pending_approval"),
                },
            )
        })
        .collect())
}

/// Pending approval count for a single system.
pub async fn pending_approval_count_for_system(
    pool: &PgPool,
    system_id: Uuid,
) -> Result<i64> {
    let row = sqlx::query(
        "SELECT COUNT(*) as count FROM deployment_approval_requests WHERE system_id = $1 AND status = 'pending'",
    )
    .bind(system_id)
    .fetch_one(pool)
    .await
    .context("pending approval count")?;
    Ok(row.get::<i64, _>("count"))
}

/// List pending approval requests for a given environment (for detail panel).
pub async fn list_pending_for_environment(
    pool: &PgPool,
    environment_id: Uuid,
) -> Result<Vec<DeploymentApprovalRequest>> {
    sqlx::query_as::<_, DeploymentApprovalRequest>(
        r#"
        SELECT * FROM deployment_approval_requests
        WHERE environment_id = $1 AND status = 'pending'
        ORDER BY requested_at ASC
        "#,
    )
    .bind(environment_id)
    .fetch_all(pool)
    .await
    .context("list pending approvals for environment")
}
