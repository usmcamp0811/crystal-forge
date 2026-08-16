use crate::api::models::UpdateNotificationPreferences;
use crate::models::user_notifications::{UserNotification, UserNotificationPreferences};
use sqlx::PgPool;
use uuid::Uuid;

pub async fn materialize_attention_notifications_for_user(
    pool: &PgPool,
    user_id: Uuid,
    email_delivery_permitted: bool,
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
                ao.category AS source_type,
                ao.subject_id,
                'attention_occurrence' AS identity_type,
                ao.id::text AS identity_id,
                ao.opened_at,
                COALESCE((p.delivery_channel IN ('in_app', 'both') AND ao.opened_at >= CASE ao.category
                    WHEN 'builds' THEN p.build_failures_in_app_enabled_at
                    WHEN 'evals' THEN p.policy_violations_in_app_enabled_at
                    WHEN 'cves' THEN p.critical_cves_in_app_enabled_at
                    WHEN 'systems' THEN p.heartbeat_lost_in_app_enabled_at
                END), FALSE) AS in_app_visible,
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
            WHERE ao.opened_at >= p.initialized_at
              AND ao.category IN ('builds', 'evals', 'cves', 'systems')
               AND p.delivery_channel IN ('in_app', 'email', 'both')
               AND (
                    (ao.category = 'builds' AND p.build_failures)
                 OR (ao.category = 'evals' AND p.policy_violations)
                 OR (ao.category = 'cves' AND p.critical_cves)
                 OR (ao.category = 'systems' AND p.heartbeat_lost)
               )
              AND (
                    (
                        p.delivery_channel IN ('in_app', 'both')
                        AND ao.opened_at >= CASE ao.category
                            WHEN 'builds' THEN p.build_failures_in_app_enabled_at
                            WHEN 'evals' THEN p.policy_violations_in_app_enabled_at
                            WHEN 'cves' THEN p.critical_cves_in_app_enabled_at
                            WHEN 'systems' THEN p.heartbeat_lost_in_app_enabled_at
                        END
                    )
                 OR (
                        p.delivery_channel IN ('email', 'both')
                        AND ao.opened_at >= CASE ao.category
                            WHEN 'builds' THEN p.build_failures_email_enabled_at
                            WHEN 'evals' THEN p.policy_violations_email_enabled_at
                            WHEN 'cves' THEN p.critical_cves_email_enabled_at
                            WHEN 'systems' THEN p.heartbeat_lost_email_enabled_at
                        END
                    )
              )
               AND notification_visible_to_user($1, ao.category, ao.subject_id)
        ), deployment_eligible AS (
            SELECT
                NULL::uuid AS source_occurrence_id,
                'deploy_failures' AS category,
                'system_event' AS subject_type,
                se.id::text AS subject_id,
                'system_event' AS identity_type,
                se.id::text AS identity_id,
                se.occurred_at AS opened_at,
                COALESCE((p.delivery_channel IN ('in_app', 'both') AND se.occurred_at >= p.deploy_failures_in_app_enabled_at), FALSE) AS in_app_visible,
                'Deployment failed' AS title,
                'A deployment entered a failed terminal state.' AS summary,
                '/systems' AS route
            FROM system_events se
            JOIN systems scoped_system ON scoped_system.id = se.system_id
            CROSS JOIN prefs p
            WHERE se.event_type = 'cf_deployment_failed'
              AND se.occurred_at >= p.initialized_at
              AND p.delivery_channel IN ('in_app', 'email', 'both')
              AND p.deploy_failures
              AND (
                    (p.delivery_channel IN ('in_app', 'both') AND se.occurred_at >= p.deploy_failures_in_app_enabled_at)
                 OR (p.delivery_channel IN ('email', 'both') AND se.occurred_at >= p.deploy_failures_email_enabled_at)
              )
               AND notification_visible_to_user($1, 'system_event', se.id::text)
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

    if !email_delivery_permitted {
        return Ok(result.rows_affected());
    }

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
                END AS category,
                ao.category AS source_type,
                ao.subject_id AS source_id,
                'attention_occurrence' AS identity_type,
                ao.id::text AS identity_id,
                ao.opened_at,
                CASE ao.category
                    WHEN 'builds' THEN p.build_failures_email_enabled_at
                    WHEN 'evals' THEN p.policy_violations_email_enabled_at
                    WHEN 'cves' THEN p.critical_cves_email_enabled_at
                    WHEN 'systems' THEN p.heartbeat_lost_email_enabled_at
                END AS email_enabled_at
            FROM attention_occurrences ao
            CROSS JOIN prefs p
            WHERE ao.opened_at >= p.initialized_at
              AND ao.category IN ('builds', 'evals', 'cves', 'systems')
              AND p.delivery_channel IN ('email', 'both')
              AND (
                    (ao.category = 'builds' AND p.build_failures)
                 OR (ao.category = 'evals' AND p.policy_violations)
                 OR (ao.category = 'cves' AND p.critical_cves)
                 OR (ao.category = 'systems' AND p.heartbeat_lost)
              )
               AND notification_visible_to_user($1, ao.category, ao.subject_id)
        ), deployment_eligible AS (
            SELECT
                NULL::uuid AS source_occurrence_id,
                'deploy_failures' AS category,
                'system_event' AS subject_type,
                se.id::text AS subject_id,
                'system_event' AS identity_type,
                se.id::text AS identity_id,
                se.occurred_at AS opened_at,
                p.deploy_failures_email_enabled_at AS email_enabled_at
            FROM system_events se
            JOIN systems scoped_system ON scoped_system.id = se.system_id
            CROSS JOIN prefs p
            WHERE se.event_type = 'cf_deployment_failed'
              AND se.occurred_at >= p.initialized_at
              AND p.delivery_channel IN ('email', 'both')
              AND p.deploy_failures
               AND notification_visible_to_user($1, 'system_event', se.id::text)
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
            'immediate:' || $1::text || ':' || e.identity_type || ':' || e.identity_id
        FROM eligible e
        LEFT JOIN existing_notification n
          ON n.category = e.category
         AND (
                (e.source_occurrence_id IS NOT NULL AND n.source_occurrence_id = e.source_occurrence_id)
             OR (e.source_occurrence_id IS NULL AND n.source_type = e.source_type AND n.source_id = e.source_id)
         )
        WHERE e.category IS NOT NULL
          AND e.email_enabled_at IS NOT NULL
          AND e.opened_at >= e.email_enabled_at
        ON CONFLICT (idempotency_key) DO NOTHING
        "#,
    )
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

pub async fn materialize_all_user_notifications(
    pool: &PgPool,
    email_delivery_permitted: bool,
) -> Result<u64, sqlx::Error> {
    let users: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT DISTINCT p.user_id
                        FROM user_notification_preferences p
                        JOIN users u ON u.id = p.user_id
                        WHERE u.is_active = TRUE",
    )
    .fetch_all(pool)
    .await?;

    let mut total = 0;
    for (user_id,) in users {
        total +=
            materialize_attention_notifications_for_user(pool, user_id, email_delivery_permitted)
                .await?;
    }
    Ok(total)
}

pub async fn enqueue_due_weekly_digest_deliveries(
    pool: &PgPool,
    digest_schedule: &str,
) -> Result<u64, sqlx::Error> {
    if digest_schedule != "weekly_utc" {
        return Ok(0);
    }
    let (period_start, period_end): (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as(
            "SELECT date_trunc('week', NOW()) - INTERVAL '7 days', date_trunc('week', NOW())",
        )
        .fetch_one(pool)
        .await?;
    let users: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT p.user_id
         FROM user_notification_preferences p
         JOIN users u ON u.id = p.user_id
         WHERE u.is_active = TRUE
           AND p.weekly_digest = TRUE
           AND p.delivery_channel IN ('email', 'both')
           AND p.weekly_digest_enabled_at IS NOT NULL
           AND p.weekly_digest_enabled_at < $1",
    )
    .bind(period_end)
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
            SELECT weekly_digest, delivery_channel, weekly_digest_enabled_at
            FROM user_notification_preferences
            WHERE user_id = $1
        ), digest_items AS (
            SELECT 1
            FROM user_notifications
            WHERE user_id = $1
              AND dismissed_at IS NULL
              AND created_at >= GREATEST($2, (SELECT weekly_digest_enabled_at FROM prefs))
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
              AND weekly_digest_enabled_at IS NOT NULL
              AND weekly_digest_enabled_at < $3
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
                   heartbeat_lost, weekly_digest, delivery_channel,
                    deploy_failures_email_enabled_at, build_failures_email_enabled_at,
                    critical_cves_email_enabled_at, policy_violations_email_enabled_at,
                    heartbeat_lost_email_enabled_at,
                    deploy_failures_in_app_enabled_at, build_failures_in_app_enabled_at,
                    critical_cves_in_app_enabled_at, policy_violations_in_app_enabled_at,
                    heartbeat_lost_in_app_enabled_at, weekly_digest_enabled_at,
                    initialized_at, updated_at",
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
             deploy_failures_email_enabled_at = CASE
                 WHEN NOT COALESCE($2, deploy_failures) OR COALESCE($8, delivery_channel) NOT IN ('email', 'both') THEN NULL
                 WHEN deploy_failures_email_enabled_at IS NULL OR NOT deploy_failures OR delivery_channel NOT IN ('email', 'both') THEN NOW()
                 ELSE deploy_failures_email_enabled_at
             END,
             build_failures_email_enabled_at = CASE
                 WHEN NOT COALESCE($3, build_failures) OR COALESCE($8, delivery_channel) NOT IN ('email', 'both') THEN NULL
                 WHEN build_failures_email_enabled_at IS NULL OR NOT build_failures OR delivery_channel NOT IN ('email', 'both') THEN NOW()
                 ELSE build_failures_email_enabled_at
             END,
             critical_cves_email_enabled_at = CASE
                 WHEN NOT COALESCE($4, critical_cves) OR COALESCE($8, delivery_channel) NOT IN ('email', 'both') THEN NULL
                 WHEN critical_cves_email_enabled_at IS NULL OR NOT critical_cves OR delivery_channel NOT IN ('email', 'both') THEN NOW()
                 ELSE critical_cves_email_enabled_at
             END,
             policy_violations_email_enabled_at = CASE
                 WHEN NOT COALESCE($5, policy_violations) OR COALESCE($8, delivery_channel) NOT IN ('email', 'both') THEN NULL
                 WHEN policy_violations_email_enabled_at IS NULL OR NOT policy_violations OR delivery_channel NOT IN ('email', 'both') THEN NOW()
                 ELSE policy_violations_email_enabled_at
             END,
              heartbeat_lost_email_enabled_at = CASE
                  WHEN NOT COALESCE($6, heartbeat_lost) OR COALESCE($8, delivery_channel) NOT IN ('email', 'both') THEN NULL
                  WHEN heartbeat_lost_email_enabled_at IS NULL OR NOT heartbeat_lost OR delivery_channel NOT IN ('email', 'both') THEN NOW()
                  ELSE heartbeat_lost_email_enabled_at
              END,
              deploy_failures_in_app_enabled_at = CASE
                  WHEN NOT COALESCE($2, deploy_failures) OR COALESCE($8, delivery_channel) NOT IN ('in_app', 'both') THEN NULL
                  WHEN deploy_failures_in_app_enabled_at IS NULL OR NOT deploy_failures OR delivery_channel NOT IN ('in_app', 'both') THEN NOW()
                  ELSE deploy_failures_in_app_enabled_at
              END,
              build_failures_in_app_enabled_at = CASE
                  WHEN NOT COALESCE($3, build_failures) OR COALESCE($8, delivery_channel) NOT IN ('in_app', 'both') THEN NULL
                  WHEN build_failures_in_app_enabled_at IS NULL OR NOT build_failures OR delivery_channel NOT IN ('in_app', 'both') THEN NOW()
                  ELSE build_failures_in_app_enabled_at
              END,
              critical_cves_in_app_enabled_at = CASE
                  WHEN NOT COALESCE($4, critical_cves) OR COALESCE($8, delivery_channel) NOT IN ('in_app', 'both') THEN NULL
                  WHEN critical_cves_in_app_enabled_at IS NULL OR NOT critical_cves OR delivery_channel NOT IN ('in_app', 'both') THEN NOW()
                  ELSE critical_cves_in_app_enabled_at
              END,
              policy_violations_in_app_enabled_at = CASE
                  WHEN NOT COALESCE($5, policy_violations) OR COALESCE($8, delivery_channel) NOT IN ('in_app', 'both') THEN NULL
                  WHEN policy_violations_in_app_enabled_at IS NULL OR NOT policy_violations OR delivery_channel NOT IN ('in_app', 'both') THEN NOW()
                  ELSE policy_violations_in_app_enabled_at
              END,
              heartbeat_lost_in_app_enabled_at = CASE
                  WHEN NOT COALESCE($6, heartbeat_lost) OR COALESCE($8, delivery_channel) NOT IN ('in_app', 'both') THEN NULL
                  WHEN heartbeat_lost_in_app_enabled_at IS NULL OR NOT heartbeat_lost OR delivery_channel NOT IN ('in_app', 'both') THEN NOW()
                  ELSE heartbeat_lost_in_app_enabled_at
              END,
              weekly_digest_enabled_at = CASE
                 WHEN NOT COALESCE($7, weekly_digest) OR COALESCE($8, delivery_channel) NOT IN ('email', 'both') THEN NULL
                 WHEN weekly_digest_enabled_at IS NULL OR NOT weekly_digest OR delivery_channel NOT IN ('email', 'both') THEN NOW()
                 ELSE weekly_digest_enabled_at
             END,
             updated_at = NOW()
         WHERE user_id = $1
         RETURNING user_id, deploy_failures, build_failures, critical_cves, policy_violations,
                   heartbeat_lost, weekly_digest, delivery_channel,
                    deploy_failures_email_enabled_at, build_failures_email_enabled_at,
                    critical_cves_email_enabled_at, policy_violations_email_enabled_at,
                    heartbeat_lost_email_enabled_at,
                    deploy_failures_in_app_enabled_at, build_failures_in_app_enabled_at,
                    critical_cves_in_app_enabled_at, policy_violations_in_app_enabled_at,
                    heartbeat_lost_in_app_enabled_at, weekly_digest_enabled_at,
                    initialized_at, updated_at",
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
    cursor: Option<(chrono::DateTime<chrono::Utc>, Uuid)>,
    unread_only: bool,
) -> Result<Vec<UserNotification>, sqlx::Error> {
    let (cursor_created_at, cursor_id) = cursor.unzip();
    sqlx::query_as::<_, UserNotification>(
        "SELECT id, user_id, category, source_occurrence_id, source_type, source_id,
                title, summary, route, created_at, read_at, dismissed_at
         FROM user_notifications
         WHERE user_id = $1
           AND in_app_visible
           AND dismissed_at IS NULL
           AND ($2::timestamptz IS NULL OR (created_at, id) < ($2, $3))
           AND ($4 = FALSE OR read_at IS NULL)
           AND notification_visible_to_user($1, source_type, source_id)
         ORDER BY created_at DESC, id DESC
         LIMIT $5",
    )
    .bind(user_id)
    .bind(cursor_created_at)
    .bind(cursor_id)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::users::{User, UserType};
    use sqlx::postgres::PgPoolOptions;

    fn test_database_url() -> String {
        std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .expect(
                "CRYSTAL_FORGE_TEST_DATABASE_URL or DATABASE_URL must be set for database tests",
            )
    }

    async fn test_pool() -> PgPool {
        PgPoolOptions::new()
            .max_connections(1)
            .connect(&test_database_url())
            .await
            .expect("failed to connect to test database")
    }

    async fn create_test_user(pool: &PgPool, label: &str) -> Uuid {
        let user_id = Uuid::new_v4();
        let short_id = user_id.simple().to_string();
        crate::queries::users::create_user(
            pool,
            User {
                id: user_id,
                username: format!("n-{label}-{}", &short_id[..8]),
                first_name: Some("Notification".to_string()),
                last_name: Some("Tester".to_string()),
                email: format!("notifications-{label}-{user_id}@example.test"),
                user_type: UserType::Human,
                is_active: true,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
        )
        .await
        .expect("create test user");
        sqlx::query("INSERT INTO user_role_assignments (user_id, role) VALUES ($1, 'viewer')")
            .bind(user_id)
            .execute(pool)
            .await
            .expect("assign viewer role");
        user_id
    }

    #[tokio::test]
    #[ignore = "requires migrated CRYSTAL_FORGE_TEST_DATABASE_URL"]
    async fn user_notifications_materialize_event_after_user_creation_before_api_touch() {
        let pool = test_pool().await;
        let user_id = create_test_user(&pool, "preference-init").await;
        get_or_create_notification_preferences(&pool, user_id)
            .await
            .expect("initialize notification preferences");
        let occurrence_id = Uuid::new_v4();
        let subject_id = Uuid::new_v4().to_string();

        sqlx::query(
            "INSERT INTO attention_occurrences
                (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at)
             VALUES ($1, 'builds', 'builds', $2, $3, NOW(), NOW())",
        )
        .bind(occurrence_id)
        .bind(&subject_id)
        .bind(format!("test-notification-{occurrence_id}"))
        .execute(&pool)
        .await
        .expect("insert attention occurrence");

        let materialized = materialize_attention_notifications_for_user(&pool, user_id, true)
            .await
            .expect("materialize notifications");

        assert_eq!(materialized, 1);
        let rows = list_notifications(&pool, user_id, 20, None, false)
            .await
            .expect("list notifications");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source_occurrence_id, Some(occurrence_id));
    }

    #[tokio::test]
    #[ignore = "requires migrated CRYSTAL_FORGE_TEST_DATABASE_URL"]
    async fn user_notifications_pagination_uses_created_at_and_id_tie_breaker() {
        let pool = test_pool().await;
        let user_id = create_test_user(&pool, "pagination").await;
        let created_at = chrono::Utc::now();
        let mut ids = Vec::new();

        for index in 0..21 {
            let id = Uuid::new_v4();
            ids.push(id);
            sqlx::query(
                "INSERT INTO user_notifications
                    (id, user_id, category, source_type, source_id, title, summary, route, created_at)
                 VALUES ($1, $2, 'build_failures', 'builds', $3, $4, 'Same timestamp', '/builds', $5)",
            )
            .bind(id)
            .bind(user_id)
            .bind(format!("pagination-{id}"))
            .bind(format!("Build failed {index}"))
            .bind(created_at)
            .execute(&pool)
            .await
            .expect("insert notification");
        }

        let first_page = list_notifications(&pool, user_id, 20, None, false)
            .await
            .expect("first page");
        assert_eq!(first_page.len(), 20);

        let last = first_page.last().expect("first page last row");
        let second_page =
            list_notifications(&pool, user_id, 20, Some((last.created_at, last.id)), false)
                .await
                .expect("second page");

        assert_eq!(second_page.len(), 1);
        assert!(!first_page.iter().any(|row| row.id == second_page[0].id));
        assert!(ids.contains(&second_page[0].id));
    }

    #[tokio::test]
    #[ignore = "requires migrated CRYSTAL_FORGE_TEST_DATABASE_URL"]
    async fn notification_authorization_fails_closed_after_role_removal_and_deactivation() {
        let pool = test_pool().await;
        let user_id = create_test_user(&pool, "authorization").await;
        let build_visible: (bool,) =
            sqlx::query_as("SELECT notification_visible_to_user($1, 'builds', 'build-1')")
                .bind(user_id)
                .fetch_one(&pool)
                .await
                .expect("check build authorization");
        assert!(build_visible.0);

        let unknown_visible: (bool,) =
            sqlx::query_as("SELECT notification_visible_to_user($1, 'future_source', 'source-1')")
                .bind(user_id)
                .fetch_one(&pool)
                .await
                .expect("check unknown authorization");
        assert!(!unknown_visible.0);

        sqlx::query("DELETE FROM user_role_assignments WHERE user_id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .expect("remove role");
        let role_removed: (bool,) =
            sqlx::query_as("SELECT notification_visible_to_user($1, 'builds', 'build-1')")
                .bind(user_id)
                .fetch_one(&pool)
                .await
                .expect("check authorization after role removal");
        assert!(!role_removed.0);

        sqlx::query("UPDATE users SET is_active = FALSE WHERE id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .expect("deactivate user");
        let inactive: (bool,) =
            sqlx::query_as("SELECT notification_visible_to_user($1, 'builds', 'build-1')")
                .bind(user_id)
                .fetch_one(&pool)
                .await
                .expect("check inactive authorization");
        assert!(!inactive.0);
    }
}
