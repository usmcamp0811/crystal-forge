use crate::api::models::UpdateNotificationPreferences;
use crate::models::user_notifications::{UserNotification, UserNotificationPreferences};
use sqlx::PgPool;
use uuid::Uuid;

pub async fn materialize_attention_notifications_for_user(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        WITH prefs AS (
            INSERT INTO user_notification_preferences (user_id)
            VALUES ($1)
            ON CONFLICT (user_id) DO UPDATE SET user_id = EXCLUDED.user_id
            RETURNING *
        ), attention_eligible AS (
            SELECT
                ao.id AS source_occurrence_id,
                CASE ao.category
                    WHEN 'builds' THEN 'build_failures'
                    WHEN 'evals' THEN 'policy_violations'
                    WHEN 'cves' THEN 'critical_cves'
                    WHEN 'systems' THEN 'heartbeat_lost'
                END AS category,
                ao.subject_type,
                ao.subject_id,
                ao.opened_at,
                CASE ao.category
                    WHEN 'builds' THEN 'Build failed'
                    WHEN 'evals' THEN 'Policy or evaluation failure'
                    WHEN 'cves' THEN 'New critical CVE'
                    WHEN 'systems' THEN 'Heartbeat lost'
                    ELSE 'Notification'
                END AS title,
                CASE ao.category
                    WHEN 'builds' THEN 'A build entered a failed terminal state.'
                    WHEN 'evals' THEN 'An evaluation or policy check entered a failed state.'
                    WHEN 'cves' THEN 'A critical CVE attention episode opened.'
                    WHEN 'systems' THEN 'A system crossed an offline or lost-heartbeat threshold.'
                    ELSE 'A Crystal Forge event needs attention.'
                END AS summary,
                CASE ao.category
                    WHEN 'builds' THEN '/builds'
                    WHEN 'evals' THEN '/evaluations'
                    WHEN 'cves' THEN '/cves'
                    WHEN 'systems' THEN '/systems'
                    ELSE '/'
                END AS route
            FROM attention_occurrences ao
            CROSS JOIN prefs p
            WHERE ao.resolved_at IS NULL
              AND ao.opened_at >= p.initialized_at
              AND ao.category IN ('builds', 'evals', 'cves', 'systems')
              AND p.delivery_channel IN ('in_app', 'both')
              AND (
                    (ao.category = 'builds' AND p.build_failures)
                 OR (ao.category = 'evals' AND p.policy_violations)
                 OR (ao.category = 'cves' AND p.critical_cves)
                 OR (ao.category = 'systems' AND p.heartbeat_lost)
              )
        ), deployment_eligible AS (
            SELECT
                NULL::uuid AS source_occurrence_id,
                'deploy_failures' AS category,
                'system_event' AS subject_type,
                se.id::text AS subject_id,
                se.occurred_at AS opened_at,
                'Deployment failed' AS title,
                'A deployment entered a failed terminal state.' AS summary,
                '/systems' AS route
            FROM system_events se
            CROSS JOIN prefs p
            WHERE se.event_type = 'cf_deployment_failed'
              AND se.occurred_at >= p.initialized_at
              AND p.delivery_channel IN ('in_app', 'both')
              AND p.deploy_failures
        ), eligible AS (
            SELECT * FROM attention_eligible
            UNION ALL
            SELECT * FROM deployment_eligible
        )
        INSERT INTO user_notifications (
            user_id, category, source_occurrence_id, source_type, source_id,
            title, summary, route, created_at
        )
        SELECT $1, category, source_occurrence_id, subject_type, subject_id,
               title, summary, route, opened_at
        FROM eligible
        WHERE category IS NOT NULL
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(user_id)
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        WITH prefs AS (
            INSERT INTO user_notification_preferences (user_id)
            VALUES ($1)
            ON CONFLICT (user_id) DO UPDATE SET user_id = EXCLUDED.user_id
            RETURNING *
        ), attention_eligible AS (
            SELECT
                ao.id AS source_occurrence_id,
                CASE ao.category
                    WHEN 'builds' THEN 'build_failures'
                    WHEN 'evals' THEN 'policy_violations'
                    WHEN 'cves' THEN 'critical_cves'
                    WHEN 'systems' THEN 'heartbeat_lost'
                END AS category
            FROM attention_occurrences ao
            CROSS JOIN prefs p
            WHERE ao.resolved_at IS NULL
              AND ao.opened_at >= p.initialized_at
              AND ao.category IN ('builds', 'evals', 'cves', 'systems')
              AND p.delivery_channel IN ('email', 'both')
              AND (
                    (ao.category = 'builds' AND p.build_failures)
                 OR (ao.category = 'evals' AND p.policy_violations)
                 OR (ao.category = 'cves' AND p.critical_cves)
                 OR (ao.category = 'systems' AND p.heartbeat_lost)
              )
        ), deployment_eligible AS (
            SELECT
                NULL::uuid AS source_occurrence_id,
                'deploy_failures' AS category,
                'system_event' AS subject_type,
                se.id::text AS subject_id
            FROM system_events se
            CROSS JOIN prefs p
            WHERE se.event_type = 'cf_deployment_failed'
              AND se.occurred_at >= p.initialized_at
              AND p.delivery_channel IN ('email', 'both')
              AND p.deploy_failures
        ), eligible AS (
            SELECT * FROM attention_eligible
            UNION ALL
            SELECT * FROM deployment_eligible
        ), existing_notification AS (
            SELECT un.id, un.source_occurrence_id, un.category::text AS category
            FROM user_notifications un
            WHERE un.user_id = $1
        )
        INSERT INTO user_notification_email_deliveries (
            user_id, notification_id, delivery_type, idempotency_key
        )
        SELECT
            $1,
            n.id,
            'immediate',
            'immediate:' || $1::text || ':' || e.category || ':' || e.source_occurrence_id::text
        FROM eligible e
        LEFT JOIN existing_notification n
          ON n.source_occurrence_id = e.source_occurrence_id
         AND n.category = e.category
        WHERE e.category IS NOT NULL
        ON CONFLICT (idempotency_key) DO NOTHING
        "#,
    )
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

pub async fn enqueue_weekly_digest_delivery(
    pool: &PgPool,
    user_id: Uuid,
    period_start: chrono::DateTime<chrono::Utc>,
    period_end: chrono::DateTime<chrono::Utc>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        r#"
        WITH prefs AS (
            SELECT weekly_digest, delivery_channel
            FROM user_notification_preferences
            WHERE user_id = $1
        ), digest_items AS (
            SELECT 1
            FROM user_notifications
            WHERE user_id = $1
              AND dismissed_at IS NULL
              AND created_at >= $2
              AND created_at < $3
            LIMIT 1
        ), delivery AS (
            INSERT INTO user_notification_email_deliveries (
                user_id, delivery_type, idempotency_key
            )
            SELECT
                $1,
                'weekly_digest',
                'weekly_digest:' || $1::text || ':' || $2::text || ':' || $3::text
            FROM prefs
            WHERE weekly_digest = TRUE
              AND delivery_channel IN ('email', 'both')
              AND EXISTS (SELECT 1 FROM digest_items)
            ON CONFLICT (idempotency_key) DO NOTHING
            RETURNING id
        )
        INSERT INTO user_notification_weekly_digest_runs (
            user_id, period_start, period_end, status, delivery_id
        )
        SELECT $1, $2, $3, 'pending', id
        FROM delivery
        ON CONFLICT (user_id, period_start, period_end) DO NOTHING
        "#,
    )
    .bind(user_id)
    .bind(period_start)
    .bind(period_end)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn get_or_create_notification_preferences(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<UserNotificationPreferences, sqlx::Error> {
    sqlx::query_as::<_, UserNotificationPreferences>(
        "INSERT INTO user_notification_preferences (user_id)
         VALUES ($1)
         ON CONFLICT (user_id) DO UPDATE SET user_id = EXCLUDED.user_id
         RETURNING user_id, deploy_failures, build_failures, critical_cves, policy_violations,
                   heartbeat_lost, weekly_digest, delivery_channel, initialized_at, updated_at",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
}

pub async fn update_notification_preferences(
    pool: &PgPool,
    user_id: Uuid,
    update: &UpdateNotificationPreferences,
) -> Result<UserNotificationPreferences, sqlx::Error> {
    let current = get_or_create_notification_preferences(pool, user_id).await?;
    let delivery_channel = update
        .delivery_channel
        .map(Into::into)
        .unwrap_or(current.delivery_channel);

    sqlx::query_as::<_, UserNotificationPreferences>(
        "UPDATE user_notification_preferences
         SET deploy_failures = $2,
             build_failures = $3,
             critical_cves = $4,
             policy_violations = $5,
             heartbeat_lost = $6,
             weekly_digest = $7,
             delivery_channel = $8,
             updated_at = NOW()
         WHERE user_id = $1
         RETURNING user_id, deploy_failures, build_failures, critical_cves, policy_violations,
                   heartbeat_lost, weekly_digest, delivery_channel, initialized_at, updated_at",
    )
    .bind(user_id)
    .bind(update.deploy_failures.unwrap_or(current.deploy_failures))
    .bind(update.build_failures.unwrap_or(current.build_failures))
    .bind(update.critical_cves.unwrap_or(current.critical_cves))
    .bind(
        update
            .policy_violations
            .unwrap_or(current.policy_violations),
    )
    .bind(update.heartbeat_lost.unwrap_or(current.heartbeat_lost))
    .bind(update.weekly_digest.unwrap_or(current.weekly_digest))
    .bind(delivery_channel)
    .fetch_one(pool)
    .await
}

pub async fn list_notifications(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
    cursor_created_before: Option<chrono::DateTime<chrono::Utc>>,
    unread_only: bool,
) -> Result<Vec<UserNotification>, sqlx::Error> {
    sqlx::query_as::<_, UserNotification>(
        "SELECT id, user_id, category, source_occurrence_id, source_type, source_id,
                title, summary, route, created_at, read_at, dismissed_at
         FROM user_notifications
         WHERE user_id = $1
           AND dismissed_at IS NULL
           AND ($2::timestamptz IS NULL OR created_at < $2)
           AND ($3 = FALSE OR read_at IS NULL)
         ORDER BY created_at DESC, id DESC
         LIMIT $4",
    )
    .bind(user_id)
    .bind(cursor_created_before)
    .bind(unread_only)
    .bind(limit.clamp(1, 50))
    .fetch_all(pool)
    .await
}

pub async fn unread_notification_count(pool: &PgPool, user_id: Uuid) -> Result<i64, sqlx::Error> {
    let (count,) = sqlx::query_as::<_, (i64,)>(
        "SELECT COUNT(*)
         FROM user_notifications
         WHERE user_id = $1 AND read_at IS NULL AND dismissed_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    Ok(count)
}

pub async fn mark_notification_read(
    pool: &PgPool,
    user_id: Uuid,
    notification_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE user_notifications
         SET read_at = COALESCE(read_at, NOW())
         WHERE id = $1 AND user_id = $2 AND dismissed_at IS NULL",
    )
    .bind(notification_id)
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn mark_all_notifications_read(pool: &PgPool, user_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE user_notifications
         SET read_at = COALESCE(read_at, NOW())
         WHERE user_id = $1 AND read_at IS NULL AND dismissed_at IS NULL",
    )
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn dismiss_notification(
    pool: &PgPool,
    user_id: Uuid,
    notification_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE user_notifications
         SET dismissed_at = COALESCE(dismissed_at, NOW())
         WHERE id = $1 AND user_id = $2",
    )
    .bind(notification_id)
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}
