use crate::models::system_events::SystemEventType;
use crate::models::system_states::SystemState;
use crate::queries::systems::BootIdChange;
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ObservedSystemState {
    pub system_id: Uuid,
    pub boot_id: Option<String>,
    pub generation: Option<i32>,
    pub store_path: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PendingSystemDeployment {
    pub id: Uuid,
    pub system_id: Uuid,
    pub target_store_path: String,
    pub status: String,
    pub source: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub applying_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SystemDeploymentProgressRow {
    pub id: Uuid,
    pub target_store_path: String,
    pub status: String,
    pub source: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub applying_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub target_commit: Option<String>,
    pub target_generation: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SystemEventHistoryRow {
    pub id: Uuid,
    pub event_type: String,
    pub event_rank: i16,
    pub occurred_at: DateTime<Utc>,
    pub observed_at: DateTime<Utc>,
    pub correlation_id: Option<Uuid>,
    pub previous_generation: Option<i64>,
    pub new_generation: Option<i64>,
    pub previous_store_path: Option<String>,
    pub new_store_path: Option<String>,
    pub previous_boot_id: Option<String>,
    pub new_boot_id: Option<String>,
    pub deployment_id: Option<Uuid>,
    pub desired_target_id: Option<Uuid>,
    pub source: String,
    pub actor: Option<String>,
    pub metadata: serde_json::Value,
    pub system_configuration_name: Option<String>,
    pub commit_hash: Option<String>,
    pub flake_name: Option<String>,
    pub flake_repo_url: Option<String>,
}

#[derive(Debug, Clone)]
struct PendingSystemEvent {
    event_type: SystemEventType,
    dedupe_key: String,
    previous_generation: Option<i64>,
    new_generation: Option<i64>,
    previous_store_path: Option<String>,
    new_store_path: Option<String>,
    previous_boot_id: Option<String>,
    new_boot_id: Option<String>,
    deployment_id: Option<Uuid>,
    desired_target_id: Option<Uuid>,
    source: &'static str,
    actor: Option<&'static str>,
    metadata: serde_json::Value,
}

const MATCH_PENDING_DEPLOYMENT_SQL: &str = r#"
        SELECT
            id,
            system_id,
            target_store_path,
            status,
            source,
            issued_at,
            expires_at,
            delivered_at,
            applying_at,
            completed_at
        FROM pending_system_deployments
        WHERE system_id = $1
          AND target_store_path = $2
          AND status = 'pending'
          AND expires_at > NOW()
        ORDER BY issued_at DESC
        LIMIT 1
        FOR UPDATE
        "#;

pub fn deployment_progress_stage(
    status: &str,
    expires_at: DateTime<Utc>,
    delivered_at: Option<DateTime<Utc>>,
    applying_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Option<&'static str> {
    match status {
        "pending" if expires_at <= now => None,
        "pending" if applying_at.is_some() => Some("applying"),
        "pending" if delivered_at.is_some() => Some("picked_up"),
        "pending" => Some("queued"),
        "succeeded" => Some("activated"),
        "failed" => Some("failed"),
        _ => None,
    }
}

pub fn deployment_progress_kind(source: &str) -> &'static str {
    if source.starts_with("manual_rollback") || source.contains("rollback") {
        "rollback"
    } else {
        "deploy"
    }
}

fn meaningful_generation_or_store_changed(
    previous_generation: Option<i32>,
    current_generation: Option<i32>,
    previous_store_path: Option<&str>,
    current_store_path: Option<&str>,
) -> bool {
    let store_changed = match (previous_store_path, current_store_path) {
        (Some(previous), Some(current)) => previous != current,
        _ => false,
    };

    let generation_changed = match (previous_generation, current_generation) {
        (Some(previous), Some(current)) => previous != current,
        _ => false,
    };

    store_changed || generation_changed
}

pub async fn lock_observed_system_state_by_hostname_tx(
    tx: &mut Transaction<'_, Postgres>,
    hostname: &str,
) -> Result<Option<ObservedSystemState>> {
    let row = sqlx::query_as::<_, ObservedSystemState>(
        r#"
        WITH locked_system AS (
            SELECT id, hostname, boot_id
            FROM systems
            WHERE hostname = $1
            FOR UPDATE
        )
        SELECT
            s.id AS system_id,
            s.boot_id,
            latest.generation,
            latest.store_path
        FROM locked_system s
        LEFT JOIN LATERAL (
            SELECT generation, store_path
            FROM system_states
            WHERE hostname = s.hostname
            ORDER BY timestamp DESC, id DESC
            LIMIT 1
        ) latest ON TRUE
        "#,
    )
    .bind(hostname)
    .fetch_optional(&mut **tx)
    .await?;

    Ok(row)
}

pub async fn set_pending_deployment_target_tx(
    tx: &mut Transaction<'_, Postgres>,
    system_id: Uuid,
    desired_target: Option<&str>,
    source: &str,
) -> Result<Option<Uuid>> {
    expire_stale_pending_deployments_tx(tx, system_id).await?;

    sqlx::query(
        r#"
        UPDATE pending_system_deployments
        SET status = 'superseded', completed_at = NOW()
        WHERE system_id = $1
          AND status = 'pending'
          AND target_store_path IS DISTINCT FROM $2
        "#,
    )
    .bind(system_id)
    .bind(desired_target)
    .execute(&mut **tx)
    .await?;

    let Some(target) = desired_target.filter(|target| target.starts_with("/nix/store/")) else {
        return Ok(None);
    };

    let expires_at = Utc::now() + Duration::hours(2);
    let id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO pending_system_deployments (
            system_id, target_store_path, source, expires_at, metadata
        )
        SELECT $1, $2, $3, $4, $5
        WHERE NOT EXISTS (
            SELECT 1
            FROM pending_system_deployments
            WHERE system_id = $1
              AND target_store_path = $2
              AND status = 'pending'
              AND expires_at > NOW()
        )
        RETURNING id
        "#,
    )
    .bind(system_id)
    .bind(target)
    .bind(source)
    .bind(expires_at)
    .bind(json!({ "desired_target": target }))
    .fetch_optional(&mut **tx)
    .await?;

    if let Some(id) = id {
        return Ok(Some(id));
    }

    let existing = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT id
        FROM pending_system_deployments
        WHERE system_id = $1
          AND target_store_path = $2
          AND status = 'pending'
          AND expires_at > NOW()
        ORDER BY issued_at DESC
        LIMIT 1
        "#,
    )
    .bind(system_id)
    .bind(target)
    .fetch_optional(&mut **tx)
    .await?;

    Ok(existing)
}

pub async fn set_pending_deployment_target(
    pool: &PgPool,
    system_id: Uuid,
    desired_target: Option<&str>,
    source: &str,
) -> Result<Option<Uuid>> {
    let mut tx = pool.begin().await?;
    let result =
        set_pending_deployment_target_tx(&mut tx, system_id, desired_target, source).await?;
    tx.commit().await?;
    Ok(result)
}

pub async fn mark_pending_deployment_delivered(
    pool: &PgPool,
    system_id: Uuid,
    target_store_path: &str,
) -> Result<u64> {
    let result = sqlx::query(
        r#"
        UPDATE pending_system_deployments
        SET delivered_at = NOW()
        WHERE system_id = $1
          AND target_store_path = $2
          AND status = 'pending'
          AND delivered_at IS NULL
        "#,
    )
    .bind(system_id)
    .bind(target_store_path)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

pub async fn mark_pending_deployment_applying_tx(
    tx: &mut Transaction<'_, Postgres>,
    system_id: Uuid,
    target_store_path: &str,
) -> Result<Option<PendingSystemDeployment>> {
    let row = find_matching_pending_deployment_tx(tx, system_id, Some(target_store_path)).await?;
    let Some(row) = row else {
        return Ok(None);
    };

    sqlx::query(
        r#"
        UPDATE pending_system_deployments
        SET applying_at = COALESCE(applying_at, NOW()),
            delivered_at = COALESCE(delivered_at, NOW())
        WHERE id = $1
          AND status = 'pending'
        "#,
    )
    .bind(row.id)
    .execute(&mut **tx)
    .await?;

    Ok(Some(row))
}

pub async fn insert_deployment_started_event_tx(
    tx: &mut Transaction<'_, Postgres>,
    pending: &PendingSystemDeployment,
) -> Result<()> {
    let event = PendingSystemEvent {
        event_type: SystemEventType::CfDeploymentStarted,
        dedupe_key: pending.id.to_string(),
        previous_generation: None,
        new_generation: None,
        previous_store_path: None,
        new_store_path: Some(pending.target_store_path.clone()),
        previous_boot_id: None,
        new_boot_id: None,
        deployment_id: Some(pending.id),
        desired_target_id: Some(pending.id),
        source: "agent_report",
        actor: Some("agent"),
        metadata: json!({
            "target_store_path": pending.target_store_path,
            "pending_deployment_id": pending.id,
        }),
    };

    insert_system_event_tx(tx, pending.system_id, pending.id, Utc::now(), &event).await
}

pub async fn get_system_deployment_progress_row(
    pool: &PgPool,
    system_id: Uuid,
) -> Result<Option<SystemDeploymentProgressRow>> {
    let row = sqlx::query_as::<_, SystemDeploymentProgressRow>(
        r#"
        SELECT
            psd.id,
            psd.target_store_path,
            psd.status,
            psd.source,
            psd.issued_at,
            psd.expires_at,
            psd.delivered_at,
            psd.applying_at,
            psd.completed_at,
            c.git_commit_hash AS target_commit,
            NULLIF(psd.metadata->>'target_generation', '')::bigint AS target_generation
        FROM pending_system_deployments psd
        LEFT JOIN derivations d
          ON d.store_path = psd.target_store_path
          OR d.expected_store_path = psd.target_store_path
        LEFT JOIN commits c ON c.id = d.commit_id
        WHERE psd.system_id = $1
          AND (
            (psd.status = 'pending' AND psd.expires_at > NOW())
            OR (psd.status IN ('succeeded', 'failed') AND psd.completed_at > NOW() - INTERVAL '2 minutes')
          )
        ORDER BY
          CASE WHEN psd.status = 'pending' THEN 0 ELSE 1 END,
          psd.issued_at DESC,
          psd.id DESC
        LIMIT 1
        "#,
    )
    .bind(system_id)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

#[cfg(test)]
mod deployment_progress_tests {
    use super::*;

    #[test]
    fn deployment_progress_stage_derives_pending_lifecycle() {
        let now = Utc::now();
        let future = now + Duration::minutes(10);

        assert_eq!(
            deployment_progress_stage("pending", future, None, None, now),
            Some("queued")
        );
        assert_eq!(
            deployment_progress_stage("pending", future, Some(now), None, now),
            Some("picked_up")
        );
        assert_eq!(
            deployment_progress_stage("pending", future, Some(now), Some(now), now),
            Some("applying")
        );
    }

    #[test]
    fn deployment_progress_stage_handles_terminal_and_expired_rows() {
        let now = Utc::now();
        let future = now + Duration::minutes(10);
        let past = now - Duration::minutes(1);

        assert_eq!(
            deployment_progress_stage("succeeded", future, None, None, now),
            Some("activated")
        );
        assert_eq!(
            deployment_progress_stage("failed", future, None, None, now),
            Some("failed")
        );
        assert_eq!(
            deployment_progress_stage("pending", past, None, None, now),
            None
        );
        assert_eq!(
            deployment_progress_stage("superseded", future, None, None, now),
            None
        );
    }

    #[test]
    fn deployment_progress_kind_distinguishes_rollbacks() {
        assert_eq!(deployment_progress_kind("manual_deploy"), "deploy");
        assert_eq!(deployment_progress_kind("auto_desired_target"), "deploy");
        assert_eq!(
            deployment_progress_kind("manual_rollback_commit"),
            "rollback"
        );
        assert_eq!(
            deployment_progress_kind("manual_rollback_generation"),
            "rollback"
        );
    }
}

async fn expire_stale_pending_deployments_tx(
    tx: &mut Transaction<'_, Postgres>,
    system_id: Uuid,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE pending_system_deployments
        SET status = 'expired', completed_at = NOW()
        WHERE system_id = $1
          AND status = 'pending'
          AND expires_at <= NOW()
        "#,
    )
    .bind(system_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn find_matching_pending_deployment_tx(
    tx: &mut Transaction<'_, Postgres>,
    system_id: Uuid,
    store_path: Option<&str>,
) -> Result<Option<PendingSystemDeployment>> {
    let Some(store_path) = store_path else {
        return Ok(None);
    };

    let row = sqlx::query_as::<_, PendingSystemDeployment>(MATCH_PENDING_DEPLOYMENT_SQL)
        .bind(system_id)
        .bind(store_path)
        .fetch_optional(&mut **tx)
        .await?;

    Ok(row)
}

async fn mark_pending_deployment_succeeded_tx(
    tx: &mut Transaction<'_, Postgres>,
    deployment_id: Uuid,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE pending_system_deployments
        SET status = 'succeeded', completed_at = NOW()
        WHERE id = $1 AND status = 'pending'
        "#,
    )
    .bind(deployment_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_system_event_tx(
    tx: &mut Transaction<'_, Postgres>,
    system_id: Uuid,
    correlation_id: Uuid,
    occurred_at: DateTime<Utc>,
    event: &PendingSystemEvent,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO system_events (
            system_id, event_type, event_rank, dedupe_key, correlation_id,
            occurred_at, previous_generation, new_generation,
            previous_store_path, new_store_path,
            previous_boot_id, new_boot_id,
            deployment_id, desired_target_id, source, actor, metadata
        ) VALUES (
            $1, $2, $3, $4, $5,
            $6, $7, $8,
            $9, $10,
            $11, $12,
            $13, $14, $15, $16, $17
        )
        ON CONFLICT (system_id, event_type, dedupe_key) DO NOTHING
        "#,
    )
    .bind(system_id)
    .bind(event.event_type.as_str())
    .bind(event.event_type.rank())
    .bind(&event.dedupe_key)
    .bind(correlation_id)
    .bind(occurred_at)
    .bind(event.previous_generation)
    .bind(event.new_generation)
    .bind(&event.previous_store_path)
    .bind(&event.new_store_path)
    .bind(&event.previous_boot_id)
    .bind(&event.new_boot_id)
    .bind(event.deployment_id)
    .bind(event.desired_target_id)
    .bind(event.source)
    .bind(event.actor)
    .bind(&event.metadata)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub async fn record_report_events_tx(
    tx: &mut Transaction<'_, Postgres>,
    previous: Option<&ObservedSystemState>,
    payload: &SystemState,
    boot_id_change: Option<BootIdChange>,
    restart_type: Option<&str>,
) -> Result<()> {
    let Some(previous) = previous else {
        return Ok(());
    };

    expire_stale_pending_deployments_tx(tx, previous.system_id).await?;

    let occurred_at = payload.timestamp.unwrap_or_else(Utc::now);
    let correlation_id = Uuid::new_v4();
    let previous_generation = previous.generation.map(i64::from);
    let new_generation = payload.generation.map(i64::from);
    let previous_store_path = previous.store_path.clone();
    let new_store_path = payload.store_path.clone();

    let generation_or_store_changed = meaningful_generation_or_store_changed(
        previous.generation,
        payload.generation,
        previous.store_path.as_deref(),
        payload.store_path.as_deref(),
    );

    let mut events = Vec::new();

    if matches!(boot_id_change, Some(BootIdChange::Changed)) {
        if let Some(new_boot_id) = payload.boot_id.clone() {
            events.push(PendingSystemEvent {
                event_type: SystemEventType::SystemReboot,
                dedupe_key: format!("system_reboot:{new_boot_id}"),
                previous_generation,
                new_generation,
                previous_store_path: previous_store_path.clone(),
                new_store_path: new_store_path.clone(),
                previous_boot_id: previous.boot_id.clone(),
                new_boot_id: Some(new_boot_id),
                deployment_id: None,
                desired_target_id: None,
                source: "agent_report",
                actor: Some("agent"),
                metadata: json!({ "change_reason": payload.change_reason.as_str() }),
            });
        }
    }

    if generation_or_store_changed {
        let pending = find_matching_pending_deployment_tx(
            tx,
            previous.system_id,
            payload.store_path.as_deref(),
        )
        .await?;

        if let Some(pending) = pending {
            mark_pending_deployment_succeeded_tx(tx, pending.id).await?;
            events.push(PendingSystemEvent {
                event_type: SystemEventType::CfDeploymentSucceeded,
                dedupe_key: format!("cf_deployment_succeeded:{}", pending.id),
                previous_generation,
                new_generation,
                previous_store_path: previous_store_path.clone(),
                new_store_path: new_store_path.clone(),
                previous_boot_id: previous.boot_id.clone(),
                new_boot_id: payload.boot_id.clone(),
                deployment_id: Some(pending.id),
                desired_target_id: Some(pending.id),
                source: "pending_desired_target",
                actor: Some("crystal-forge"),
                metadata: json!({
                    "change_reason": payload.change_reason.as_str(),
                    "target_store_path": pending.target_store_path,
                    "pending_source": pending.source,
                }),
            });
        } else {
            let old = previous.store_path.as_deref().unwrap_or("unknown");
            let new = payload.store_path.as_deref().unwrap_or("unknown");
            let old_generation = previous
                .generation
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let new_generation_key = payload
                .generation
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            events.push(PendingSystemEvent {
                event_type: SystemEventType::LocalRebuildDetected,
                dedupe_key: format!(
                    "local_rebuild:{old_generation}:{old}->{new_generation_key}:{new}"
                ),
                previous_generation,
                new_generation,
                previous_store_path: previous_store_path.clone(),
                new_store_path: new_store_path.clone(),
                previous_boot_id: previous.boot_id.clone(),
                new_boot_id: payload.boot_id.clone(),
                deployment_id: None,
                desired_target_id: None,
                source: "agent_report",
                actor: Some("on-host"),
                metadata: json!({ "change_reason": payload.change_reason.as_str() }),
            });
        }
    } else if payload.change_reason == "startup" && restart_type == Some("agent_restart") {
        let boot = payload.boot_id.as_deref().unwrap_or("unknown");
        let store = payload.store_path.as_deref().unwrap_or("unknown");
        events.push(PendingSystemEvent {
            event_type: SystemEventType::AgentRestart,
            dedupe_key: format!("agent_restart:{boot}:{store}"),
            previous_generation,
            new_generation,
            previous_store_path: previous_store_path.clone(),
            new_store_path: new_store_path.clone(),
            previous_boot_id: previous.boot_id.clone(),
            new_boot_id: payload.boot_id.clone(),
            deployment_id: None,
            desired_target_id: None,
            source: "agent_report",
            actor: Some("agent"),
            metadata: json!({
                "change_reason": payload.change_reason.as_str(),
                "dedupe_semantics": "coalesced by boot_id and store path until durable agent instance identity is available"
            }),
        });
    }

    for event in &events {
        insert_system_event_tx(tx, previous.system_id, correlation_id, occurred_at, event).await?;
    }

    Ok(())
}

pub async fn list_system_event_history_rows(
    pool: &PgPool,
    system_id: Uuid,
    limit: i64,
) -> Result<Vec<SystemEventHistoryRow>> {
    let rows = sqlx::query_as::<_, SystemEventHistoryRow>(
        r#"
        SELECT
            se.id,
            se.event_type,
            se.event_rank,
            se.occurred_at,
            se.observed_at,
            se.correlation_id,
            se.previous_generation,
            se.new_generation,
            se.previous_store_path,
            se.new_store_path,
            se.previous_boot_id,
            se.new_boot_id,
            se.deployment_id,
            se.desired_target_id,
            se.source,
            se.actor,
            se.metadata,
            COALESCE(NULLIF(s.system_configuration_name, ''), s.hostname) AS system_configuration_name,
            c.git_commit_hash AS commit_hash,
            f.name AS flake_name,
            f.repo_url AS flake_repo_url
        FROM system_events se
        JOIN systems s ON s.id = se.system_id
        LEFT JOIN derivations d
          ON se.new_store_path = COALESCE(d.store_path, d.expected_store_path)
         AND d.derivation_type = 'nixos'
         AND d.derivation_name = COALESCE(NULLIF(s.system_configuration_name, ''), s.hostname)
        LEFT JOIN commits c ON c.id = d.commit_id
        LEFT JOIN flakes f ON f.id = c.flake_id
        WHERE se.system_id = $1
        ORDER BY se.occurred_at DESC, se.observed_at DESC, se.correlation_id DESC, se.event_rank ASC, se.id DESC
        LIMIT $2
        "#,
    )
    .bind(system_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_type_strings_match_api_contract() {
        assert_eq!(SystemEventType::SystemReboot.as_str(), "system_reboot");
        assert_eq!(SystemEventType::AgentRestart.as_str(), "agent_restart");
        assert_eq!(
            SystemEventType::CfDeploymentSucceeded.as_str(),
            "cf_deployment_succeeded"
        );
        assert_eq!(
            SystemEventType::CfDeploymentFailed.as_str(),
            "cf_deployment_failed"
        );
        assert_eq!(
            SystemEventType::LocalRebuildDetected.as_str(),
            "local_rebuild_detected"
        );
    }

    #[test]
    fn event_ranks_put_config_transition_before_restart_for_same_report() {
        assert_eq!(SystemEventType::CfDeploymentSucceeded.rank(), 10);
        assert_eq!(SystemEventType::LocalRebuildDetected.rank(), 10);
        assert_eq!(SystemEventType::SystemReboot.rank(), 20);
        assert_eq!(SystemEventType::AgentRestart.rank(), 30);
    }

    #[test]
    fn matching_pending_deployment_sql_excludes_succeeded_contexts() {
        assert!(MATCH_PENDING_DEPLOYMENT_SQL.contains("status = 'pending'"));
        assert!(!MATCH_PENDING_DEPLOYMENT_SQL.contains("succeeded"));
        assert!(!MATCH_PENDING_DEPLOYMENT_SQL.contains("status IN"));
    }

    #[test]
    fn missing_generation_or_store_path_does_not_invent_transition() {
        assert!(!meaningful_generation_or_store_changed(
            Some(5058),
            None,
            Some("/nix/store/current-system"),
            Some("/nix/store/current-system"),
        ));
        assert!(!meaningful_generation_or_store_changed(
            Some(5058),
            Some(5058),
            Some("/nix/store/current-system"),
            None,
        ));
        assert!(!meaningful_generation_or_store_changed(
            Some(5058),
            None,
            Some("/nix/store/current-system"),
            None,
        ));
    }

    #[test]
    fn present_generation_or_store_path_change_is_transition() {
        assert!(meaningful_generation_or_store_changed(
            Some(5058),
            Some(5059),
            Some("/nix/store/current-system"),
            Some("/nix/store/current-system"),
        ));
        assert!(meaningful_generation_or_store_changed(
            Some(5058),
            Some(5058),
            Some("/nix/store/old-system"),
            Some("/nix/store/new-system"),
        ));
    }

    #[test]
    fn setting_pending_target_expires_stale_rows_and_ignores_expired_same_target_rows() {
        let source = include_str!("system_events.rs");
        assert!(
            source.contains("expire_stale_pending_deployments_tx(tx, system_id).await?"),
            "set_pending_deployment_target_tx must expire stale pending rows before insert"
        );
        assert!(
            source.contains("AND expires_at > NOW()\n        )\n        RETURNING id"),
            "same-target insert guard must ignore expired pending rows"
        );
        assert!(
            source.contains("AND expires_at > NOW()\n        ORDER BY issued_at DESC"),
            "same-target fallback SELECT must ignore expired pending rows"
        );
    }
}
