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
        ), authz AS (
            SELECT EXISTS (
                SELECT 1 FROM user_role_assignments
                WHERE user_id = $1 AND role = 'admin'
            ) AS is_admin
        ), attention_eligible AS (
            SELECT
                ao.id AS source_occurrence_id,
                CASE ao.category
                    WHEN 'builds' THEN 'build_failures'
                    WHEN 'evals' THEN 'policy_violations'
                    WHEN 'cves' THEN 'critical_cves'
                    WHEN 'systems' THEN 'heartbeat_lost'
                END AS category,
                ao.category AS source_type,
                ao.subject_id,
                ao.opened_at,
                p.delivery_channel IN ('in_app', 'both') AS in_app_visible,
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
            CROSS JOIN authz a
            LEFT JOIN systems scoped_system
              ON ao.category = 'systems'
             AND scoped_system.id::text = ao.subject_id
            WHERE ao.resolved_at IS NULL
              AND ao.opened_at >= p.initialized_at
              AND ao.category IN ('builds', 'evals', 'cves', 'systems')
              AND p.delivery_channel IN ('in_app', 'email', 'both')
              AND (
                    (ao.category = 'builds' AND p.build_failures)
                 OR (ao.category = 'evals' AND p.policy_violations)
                 OR (ao.category = 'cves' AND p.critical_cves)
                 OR (ao.category = 'systems' AND p.heartbeat_lost)
              )
              AND (
                    a.is_admin
                 OR ao.category <> 'systems'
                 OR EXISTS (
                    SELECT 1
                    FROM user_environment_memberships uem
                    WHERE uem.user_id = $1
                      AND uem.environment_id = scoped_system.environment_id
                 )
              )
        ), deployment_eligible AS (
            SELECT
                NULL::uuid AS source_occurrence_id,
                'deploy_failures' AS category,
                'system_event' AS subject_type,
                se.id::text AS subject_id,
                se.occurred_at AS opened_at,
                p.delivery_channel IN ('in_app', 'both') AS in_app_visible,
                'Deployment failed' AS title,
                'A deployment entered a failed terminal state.' AS summary,
                '/systems' AS route
            FROM system_events se
            JOIN systems scoped_system ON scoped_system.id = se.system_id
            CROSS JOIN prefs p
            CROSS JOIN authz a
            WHERE se.event_type = 'cf_deployment_failed'
              AND se.occurred_at >= p.initialized_at
              AND p.delivery_channel IN ('in_app', 'email', 'both')
              AND p.deploy_failures
              AND (
                    a.is_admin
                 OR EXISTS (
                    SELECT 1
                    FROM user_environment_memberships uem
                    WHERE uem.user_id = $1
                      AND uem.environment_id = scoped_system.environment_id
                 )
              )
        ), eligible AS (
            SELECT * FROM attention_eligible
            UNION ALL
            SELECT * FROM deployment_eligible
        )
        INSERT INTO user_notifications (
            user_id, category, source_occurrence_id, source_type, source_id,
            title, summary, route, in_app_visible, created_at
        )
        SELECT $1, category, source_occurrence_id, source_type, subject_id,
               title, summary, route, in_app_visible, opened_at
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
        ), authz AS (
            SELECT EXISTS (
                SELECT 1 FROM user_role_assignments
                WHERE user_id = $1 AND role = 'admin'
            ) AS is_admin
        ), attention_eligible AS (
            SELECT
                ao.id AS source_occurrence_id,
                CASE ao.category
                    WHEN 'builds' THEN 'build_failures'
                    WHEN 'evals' THEN 'policy_violations'
                    WHEN 'cves' THEN 'critical_cves'
                    WHEN 'systems' THEN 'heartbeat_lost'
                END AS category,
                ao.category AS source_type,
                ao.subject_id AS source_id
            FROM attention_occurrences ao
            CROSS JOIN prefs p
            CROSS JOIN authz a
            LEFT JOIN systems scoped_system
              ON ao.category = 'systems'
             AND scoped_system.id::text = ao.subject_id
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
              AND (
                    a.is_admin
                 OR ao.category <> 'systems'
                 OR EXISTS (
                    SELECT 1
                    FROM user_environment_memberships uem
                    WHERE uem.user_id = $1
                      AND uem.environment_id = scoped_system.environment_id
                 )
              )
        ), deployment_eligible AS (
            SELECT
                NULL::uuid AS source_occurrence_id,
                'deploy_failures' AS category,
                'system_event' AS subject_type,
                se.id::text AS subject_id
            FROM system_events se
            JOIN systems scoped_system ON scoped_system.id = se.system_id
            CROSS JOIN prefs p
            CROSS JOIN authz a
            WHERE se.event_type = 'cf_deployment_failed'
              AND se.occurred_at >= p.initialized_at
              AND p.delivery_channel IN ('email', 'both')
              AND p.deploy_failures
              AND (
                    a.is_admin
                 OR EXISTS (
                    SELECT 1
                    FROM user_environment_memberships uem
                    WHERE uem.user_id = $1
                      AND uem.environment_id = scoped_system.environment_id
                 )
              )
        ), eligible AS (
            SELECT * FROM attention_eligible
            UNION ALL
            SELECT * FROM deployment_eligible
        ), existing_notification AS (
            SELECT un.id, un.source_occurrence_id, un.category::text AS category,
                   un.source_type, un.source_id
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
            'immediate:' || $1::text || ':' || e.source_type || ':' || e.source_id
        FROM eligible e
        LEFT JOIN existing_notification n
          ON n.source_type = e.source_type
         AND n.source_id = e.source_id
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

pub async fn materialize_all_user_notifications(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let users: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT DISTINCT user_id FROM user_notification_preferences",
    )
    .fetch_all(pool)
    .await?;

    let mut total = 0;
    for (user_id,) in users {
        total += materialize_attention_notifications_for_user(pool, user_id).await?;
    }
    Ok(total)
}

pub async fn enqueue_due_weekly_digest_deliveries(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let period_end = chrono::Utc::now();
    let period_start = period_end - chrono::Duration::days(7);
    let users: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT user_id
         FROM user_notification_preferences
         WHERE weekly_digest = TRUE
           AND delivery_channel IN ('email', 'both')",
    )
    .fetch_all(pool)
    .await?;

    let mut total = 0;
    for (user_id,) in users {
        if enqueue_weekly_digest_delivery(pool, user_id, period_start, period_end).await? {
            total += 1;
        }
    }
    Ok(total)
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
              AND notification_visible_to_user($1, source_type, source_id)
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
    get_or_create_notification_preferences(pool, user_id).await?;
    let delivery_channel: Option<crate::models::user_notifications::NotificationDeliveryChannel> =
        update.delivery_channel.map(Into::into);

    sqlx::query_as::<_, UserNotificationPreferences>(
        "UPDATE user_notification_preferences
         SET deploy_failures = COALESCE($2, deploy_failures),
             build_failures = COALESCE($3, build_failures),
             critical_cves = COALESCE($4, critical_cves),
             policy_violations = COALESCE($5, policy_violations),
             heartbeat_lost = COALESCE($6, heartbeat_lost),
             weekly_digest = COALESCE($7, weekly_digest),
             delivery_channel = COALESCE($8, delivery_channel),
             updated_at = NOW()
         WHERE user_id = $1
         RETURNING user_id, deploy_failures, build_failures, critical_cves, policy_violations,
                   heartbeat_lost, weekly_digest, delivery_channel, initialized_at, updated_at",
    )
    .bind(user_id)
    .bind(update.deploy_failures)
    .bind(update.build_failures)
    .bind(update.critical_cves)
    .bind(update.policy_violations)
    .bind(update.heartbeat_lost)
    .bind(update.weekly_digest)
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
           AND in_app_visible
           AND dismissed_at IS NULL
           AND ($2::timestamptz IS NULL OR created_at < $2)
           AND ($3 = FALSE OR read_at IS NULL)
           AND notification_visible_to_user($1, source_type, source_id)
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
         WHERE user_id = $1
           AND in_app_visible
           AND read_at IS NULL
           AND dismissed_at IS NULL
           AND notification_visible_to_user($1, source_type, source_id)",
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
         WHERE id = $1
           AND user_id = $2
           AND in_app_visible
           AND dismissed_at IS NULL
           AND notification_visible_to_user($2, source_type, source_id)",
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
         WHERE user_id = $1
           AND in_app_visible
           AND read_at IS NULL
           AND dismissed_at IS NULL
           AND notification_visible_to_user($1, source_type, source_id)",
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
         WHERE id = $1
           AND user_id = $2
           AND in_app_visible
           AND notification_visible_to_user($2, source_type, source_id)",
    )
    .bind(notification_id)
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}
