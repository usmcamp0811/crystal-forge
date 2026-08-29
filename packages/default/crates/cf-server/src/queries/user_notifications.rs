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
            INSERT INTO user_notification_preferences (
                user_id, deploy_failures_in_app_enabled_at,
                build_failures_in_app_enabled_at,
                critical_cves_in_app_enabled_at,
                policy_violations_in_app_enabled_at, initialized_at
            )
            SELECT id, created_at, created_at, created_at, created_at, created_at
            FROM users WHERE id = $1
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
                    WHEN 'poams' THEN 'policy_violations'
                END AS category,
                CASE WHEN ao.category = 'poams' THEN 'poams' ELSE ao.category END AS source_type,
                ao.subject_id,
                'attention_occurrence' AS identity_type,
                ao.id::text AS identity_id,
                ao.opened_at,
                 COALESCE((p.delivery_channel IN ('in_app', 'both') AND ao.opened_at >= CASE ao.category
                    WHEN 'builds' THEN p.build_failures_in_app_enabled_at
                    WHEN 'evals' THEN p.policy_violations_in_app_enabled_at
                    WHEN 'cves' THEN p.critical_cves_in_app_enabled_at
                    WHEN 'systems' THEN p.heartbeat_lost_in_app_enabled_at
                    WHEN 'poams' THEN p.policy_violations_in_app_enabled_at
                     END), FALSE) AS in_app_visible,
                 COALESCE(($2 AND p.delivery_channel IN ('email', 'both') AND ao.opened_at >= CASE ao.category
                     WHEN 'builds' THEN p.build_failures_email_enabled_at
                     WHEN 'evals' THEN p.policy_violations_email_enabled_at
                     WHEN 'cves' THEN p.critical_cves_email_enabled_at
                     WHEN 'systems' THEN p.heartbeat_lost_email_enabled_at
                     WHEN 'poams' THEN p.policy_violations_email_enabled_at
                 END), FALSE) AS email_delivery_eligible,
                CASE ao.category
                    WHEN 'builds' THEN 'Build failed'
                    WHEN 'evals' THEN 'Policy or evaluation failure'
                    WHEN 'cves' THEN 'New critical CVE'
                    WHEN 'systems' THEN 'Heartbeat lost'
                    WHEN 'poams' THEN 'POAM-' || lpad(poam.human_number::text, 4, '0') || ' overdue'
                    ELSE 'Notification'
                END AS title,
                CASE ao.category
                    WHEN 'builds' THEN 'A build entered a failed terminal state.'
                    WHEN 'evals' THEN 'An evaluation or policy check entered a failed state.'
                    WHEN 'cves' THEN 'A critical CVE attention episode opened.'
                    WHEN 'systems' THEN 'A system crossed an offline or lost-heartbeat threshold.'
                    WHEN 'poams' THEN poam.title || ' passed its target date.'
                    ELSE 'A Crystal Forge event needs attention.'
                END AS summary,
                CASE ao.category
                    WHEN 'builds' THEN '/builds'
                    WHEN 'evals' THEN '/evaluations'
                    WHEN 'cves' THEN '/cves'
                    WHEN 'systems' THEN '/systems'
                    WHEN 'poams' THEN '/compliance?poam=' || poam.id::text
                    ELSE '/'
                END AS route
            FROM attention_occurrences ao
            LEFT JOIN poams poam ON ao.category = 'poams' AND poam.id::text = ao.subject_id
            CROSS JOIN prefs p
            WHERE ao.opened_at >= p.initialized_at
              AND ao.category IN ('builds', 'evals', 'cves', 'systems', 'poams')
               AND p.delivery_channel IN ('in_app', 'email', 'both')
               AND (
                    (ao.category = 'builds' AND p.build_failures)
                 OR (ao.category = 'evals' AND p.policy_violations)
                 OR (ao.category = 'cves' AND p.critical_cves)
                  OR (ao.category = 'systems' AND p.heartbeat_lost)
                  OR (ao.category = 'poams' AND p.policy_violations)
               )
              AND (
                    (
                        p.delivery_channel IN ('in_app', 'both')
                        AND ao.opened_at >= CASE ao.category
                            WHEN 'builds' THEN p.build_failures_in_app_enabled_at
                            WHEN 'evals' THEN p.policy_violations_in_app_enabled_at
                            WHEN 'cves' THEN p.critical_cves_in_app_enabled_at
                            WHEN 'systems' THEN p.heartbeat_lost_in_app_enabled_at
                            WHEN 'poams' THEN p.policy_violations_in_app_enabled_at
                        END
                    )
                 OR (
                        p.delivery_channel IN ('email', 'both')
                        AND ao.opened_at >= CASE ao.category
                            WHEN 'builds' THEN p.build_failures_email_enabled_at
                            WHEN 'evals' THEN p.policy_violations_email_enabled_at
                            WHEN 'cves' THEN p.critical_cves_email_enabled_at
                            WHEN 'systems' THEN p.heartbeat_lost_email_enabled_at
                            WHEN 'poams' THEN p.policy_violations_email_enabled_at
                        END
                    )
              )
               AND notification_visible_to_user($1, CASE WHEN ao.category = 'poams' THEN 'poams' ELSE ao.category END, ao.subject_id)
        ), poam_activity_eligible AS (
            SELECT
                activity.id AS source_occurrence_id,
                'policy_violations' AS category,
                'poams' AS source_type,
                poam.id::text AS subject_id,
                'poam_activity' AS identity_type,
                activity.id::text AS identity_id,
                activity.created_at AS opened_at,
                COALESCE((p.delivery_channel IN ('in_app', 'both')
                    AND activity.created_at >= p.policy_violations_in_app_enabled_at), FALSE) AS in_app_visible,
                COALESCE(($2 AND p.delivery_channel IN ('email', 'both')
                    AND activity.created_at >= p.policy_violations_email_enabled_at), FALSE) AS email_delivery_eligible,
                'POAM-' || lpad(poam.human_number::text, 4, '0') || ' awaiting verification' AS title,
                poam.title || ' is ready for verification.' AS summary,
                '/compliance?poam=' || poam.id::text AS route
            FROM poam_activity activity
            JOIN poams poam ON poam.id = activity.poam_id
            CROSS JOIN prefs p
            WHERE activity.kind = 'status_changed'
              AND activity.payload->>'to' = 'awaiting_verification'
              AND activity.created_at >= p.initialized_at
              AND p.policy_violations
              AND p.delivery_channel IN ('in_app', 'email', 'both')
              AND (
                    (p.delivery_channel IN ('in_app', 'both')
                     AND activity.created_at >= p.policy_violations_in_app_enabled_at)
                 OR (p.delivery_channel IN ('email', 'both')
                     AND activity.created_at >= p.policy_violations_email_enabled_at)
              )
              AND notification_visible_to_user($1, 'poams', poam.id::text)
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
                 COALESCE(($2 AND p.delivery_channel IN ('email', 'both') AND se.occurred_at >= p.deploy_failures_email_enabled_at), FALSE) AS email_delivery_eligible,
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
            SELECT * FROM poam_activity_eligible
            UNION ALL
            SELECT * FROM deployment_eligible
        )
        INSERT INTO user_notifications (
            user_id, category, source_occurrence_id, source_type, source_id,
            title, summary, route, in_app_visible, email_delivery_eligible, created_at
        )
        SELECT $1, category, source_occurrence_id, source_type, subject_id,
               title, summary, route, in_app_visible, email_delivery_eligible, opened_at
        FROM eligible
        WHERE category IS NOT NULL
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(user_id)
    .bind(email_delivery_permitted)
    .execute(pool)
    .await?;

    if !email_delivery_permitted {
        return Ok(result.rows_affected());
    }

    sqlx::query(
        r#"
        WITH prefs AS (
            INSERT INTO user_notification_preferences (
                user_id, deploy_failures_in_app_enabled_at,
                build_failures_in_app_enabled_at,
                critical_cves_in_app_enabled_at,
                policy_violations_in_app_enabled_at, initialized_at
            )
            SELECT id, created_at, created_at, created_at, created_at, created_at
            FROM users WHERE id = $1
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
                    WHEN 'poams' THEN 'policy_violations'
                END AS category,
                CASE WHEN ao.category = 'poams' THEN 'poams' ELSE ao.category END AS source_type,
                ao.subject_id AS source_id,
                'attention_occurrence' AS identity_type,
                ao.id::text AS identity_id,
                ao.opened_at,
                CASE ao.category
                    WHEN 'builds' THEN p.build_failures_email_enabled_at
                    WHEN 'evals' THEN p.policy_violations_email_enabled_at
                    WHEN 'cves' THEN p.critical_cves_email_enabled_at
                    WHEN 'systems' THEN p.heartbeat_lost_email_enabled_at
                    WHEN 'poams' THEN p.policy_violations_email_enabled_at
                END AS email_enabled_at
            FROM attention_occurrences ao
            CROSS JOIN prefs p
            WHERE ao.opened_at >= p.initialized_at
              AND ao.category IN ('builds', 'evals', 'cves', 'systems', 'poams')
              AND p.delivery_channel IN ('email', 'both')
              AND (
                    (ao.category = 'builds' AND p.build_failures)
                 OR (ao.category = 'evals' AND p.policy_violations)
                 OR (ao.category = 'cves' AND p.critical_cves)
                  OR (ao.category = 'systems' AND p.heartbeat_lost)
                  OR (ao.category = 'poams' AND p.policy_violations)
               )
               AND notification_visible_to_user($1, CASE WHEN ao.category = 'poams' THEN 'poams' ELSE ao.category END, ao.subject_id)
        ), poam_activity_eligible AS (
            SELECT
                activity.id AS source_occurrence_id,
                'policy_violations' AS category,
                'poams' AS source_type,
                poam.id::text AS source_id,
                'poam_activity' AS identity_type,
                activity.id::text AS identity_id,
                activity.created_at AS opened_at,
                p.policy_violations_email_enabled_at AS email_enabled_at
            FROM poam_activity activity
            JOIN poams poam ON poam.id = activity.poam_id
            CROSS JOIN prefs p
            WHERE activity.kind = 'status_changed'
              AND activity.payload->>'to' = 'awaiting_verification'
              AND activity.created_at >= p.initialized_at
              AND p.delivery_channel IN ('email', 'both')
              AND p.policy_violations
              AND notification_visible_to_user($1, 'poams', poam.id::text)
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
            SELECT * FROM poam_activity_eligible
            UNION ALL
            SELECT * FROM deployment_eligible
        ), existing_notification AS (
            SELECT un.id, un.source_occurrence_id, un.category::text AS category,
                   un.source_type, un.source_id, un.email_delivery_eligible
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
        JOIN existing_notification n
          ON n.category = e.category
         AND (
                (e.source_occurrence_id IS NOT NULL AND n.source_occurrence_id = e.source_occurrence_id)
             OR (e.source_occurrence_id IS NULL AND n.source_type = e.source_type AND n.source_id = e.source_id)
         )
        WHERE e.category IS NOT NULL
           AND e.email_enabled_at IS NOT NULL
           AND e.opened_at >= e.email_enabled_at
           AND n.email_delivery_eligible
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
        "SELECT id
         FROM users
         WHERE is_active = TRUE",
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
              AND email_delivery_eligible
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
        "INSERT INTO user_notification_preferences (
             user_id, deploy_failures_in_app_enabled_at,
             build_failures_in_app_enabled_at,
             critical_cves_in_app_enabled_at,
             policy_violations_in_app_enabled_at, initialized_at
         )
         SELECT id, created_at, created_at, created_at, created_at, created_at
         FROM users WHERE id = $1
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

/// Returns existing notification preferences without creating or updating rows.
///
/// # Errors
///
/// Returns a database error when the preference query fails.
pub async fn get_notification_preferences(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Option<UserNotificationPreferences>, sqlx::Error> {
    sqlx::query_as::<_, UserNotificationPreferences>(
        "SELECT user_id, deploy_failures, build_failures, critical_cves, policy_violations,
                heartbeat_lost, weekly_digest, delivery_channel,
                deploy_failures_email_enabled_at, build_failures_email_enabled_at,
                critical_cves_email_enabled_at, policy_violations_email_enabled_at,
                heartbeat_lost_email_enabled_at,
                deploy_failures_in_app_enabled_at, build_failures_in_app_enabled_at,
                critical_cves_in_app_enabled_at, policy_violations_in_app_enabled_at,
                heartbeat_lost_in_app_enabled_at, weekly_digest_enabled_at,
                initialized_at, updated_at
         FROM user_notification_preferences WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
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
            .max_connections(5)
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

    async fn create_poam_fixture(pool: &PgPool, user_id: Uuid) -> Uuid {
        let token = Uuid::new_v4().simple().to_string();
        let environment_id = Uuid::new_v4();
        let system_id = Uuid::new_v4();
        let policy_id = Uuid::new_v4();
        let finding_id = Uuid::new_v4();
        let poam_id = Uuid::new_v4();
        let mut tx = pool.begin().await.expect("begin POA&M fixture transaction");

        sqlx::query("UPDATE user_role_assignments SET role = 'admin' WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .expect("grant fixture administrator role");
        sqlx::query("INSERT INTO environments (id, name) VALUES ($1, $2)")
            .bind(environment_id)
            .bind(format!("notification-poam-{token}"))
            .execute(&mut *tx)
            .await
            .expect("insert fixture environment");
        sqlx::query(
            "INSERT INTO systems (id, hostname, environment_id, public_key, derivation, is_active) \
             VALUES ($1, $2, $3, $4, '', TRUE)",
        )
        .bind(system_id)
        .bind(format!("notification-poam-{token}"))
        .bind(environment_id)
        .bind(format!("ssh-ed25519 AAAA-notification-poam-{token}"))
        .execute(&mut *tx)
        .await
        .expect("insert fixture system");
        sqlx::query(
            "INSERT INTO deployment_policies (id, name, policy_type, config, enabled) \
             VALUES ($1, $2, 'custom_check', '{\"expression\": \"true\"}', FALSE)",
        )
        .bind(policy_id)
        .bind(format!("notification-poam-{token}"))
        .execute(&mut *tx)
        .await
        .expect("insert fixture policy");
        sqlx::query(
            "INSERT INTO poam_findings (id, system_id, policy_lineage_id) VALUES ($1, $2, $3)",
        )
        .bind(finding_id)
        .bind(system_id)
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .expect("insert fixture finding");
        sqlx::query(
            "INSERT INTO poams (id, title, target_date, risk, created_by) \
             VALUES ($1, $2, CURRENT_DATE - 2, 'medium', $3)",
        )
        .bind(poam_id)
        .bind(format!("Notification POA&M {token}"))
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .expect("insert fixture POA&M");
        sqlx::query(
            "INSERT INTO poam_finding_links (poam_id, finding_id, linked_by) VALUES ($1, $2, $3)",
        )
        .bind(poam_id)
        .bind(finding_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .expect("link fixture finding");
        tx.commit().await.expect("commit POA&M fixture");
        poam_id
    }

    #[tokio::test]
    #[ignore = "requires migrated CRYSTAL_FORGE_TEST_DATABASE_URL"]
    async fn user_notifications_materialize_event_after_user_creation_before_api_touch() {
        let pool = test_pool().await;
        let user_id = create_test_user(&pool, "preference-init").await;
        sqlx::query("DELETE FROM user_notification_preferences WHERE user_id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .expect("remove initialized notification preferences");
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

        materialize_all_user_notifications(&pool, true)
            .await
            .expect("materialize all active-user notifications");

        let rows = list_notifications(&pool, user_id, 20, None, false)
            .await
            .expect("list notifications");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source_occurrence_id, Some(occurrence_id));
    }

    #[tokio::test]
    #[ignore = "requires migrated CRYSTAL_FORGE_TEST_DATABASE_URL"]
    async fn poam_notifications_deduplicate_and_preserve_inbox_state() {
        let pool = test_pool().await;
        let user_id = create_test_user(&pool, "poam-events").await;
        let poam_id = create_poam_fixture(&pool, user_id).await;
        crate::tasks::attention_reconciliation::reconcile_poam_overdue_subject(&pool, poam_id)
            .await
            .expect("reconcile overdue POA&M");

        for _ in 0..2 {
            sqlx::query(
                "INSERT INTO poam_activity (poam_id, actor_user_id, kind, payload) \
                 VALUES ($1, $2, 'status_changed', \
                    jsonb_build_object('from', 'in_progress', 'to', 'awaiting_verification'))",
            )
            .bind(poam_id)
            .bind(user_id)
            .execute(&pool)
            .await
            .expect("insert awaiting-verification activity");
        }

        let (first, second) = tokio::join!(
            materialize_attention_notifications_for_user(&pool, user_id, false),
            materialize_attention_notifications_for_user(&pool, user_id, false),
        );
        first.expect("first concurrent materialization");
        second.expect("second concurrent materialization");

        let notifications: Vec<(Uuid, String, Option<Uuid>)> = sqlx::query_as(
            "SELECT id, route, source_occurrence_id FROM user_notifications \
             WHERE user_id = $1 AND source_type = 'poams' ORDER BY created_at, id",
        )
        .bind(user_id)
        .fetch_all(&pool)
        .await
        .expect("list POA&M notifications");
        assert_eq!(notifications.len(), 3);
        assert!(notifications.iter().all(|(_, route, source_id)| {
            route == &format!("/compliance?poam={poam_id}") && source_id.is_some()
        }));

        assert!(
            mark_notification_read(&pool, user_id, notifications[0].0)
                .await
                .expect("mark POA&M notification read")
        );
        assert!(
            dismiss_notification(&pool, user_id, notifications[1].0)
                .await
                .expect("dismiss POA&M notification")
        );
        materialize_attention_notifications_for_user(&pool, user_id, false)
            .await
            .expect("repeat POA&M materialization");

        let state: (i64, i64, i64) = sqlx::query_as(
            "SELECT COUNT(*)::bigint, COUNT(read_at)::bigint, COUNT(dismissed_at)::bigint \
             FROM user_notifications WHERE user_id = $1 AND source_type = 'poams'",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("read durable POA&M inbox state");
        assert_eq!(state, (3, 1, 1));
    }

    #[tokio::test]
    #[ignore = "requires migrated CRYSTAL_FORGE_TEST_DATABASE_URL"]
    async fn notification_preference_read_does_not_initialize_missing_row() {
        let pool = test_pool().await;
        let user_id = create_test_user(&pool, "read-only-preferences").await;
        sqlx::query("DELETE FROM user_notification_preferences WHERE user_id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .expect("remove initialized notification preferences");

        let preferences = get_notification_preferences(&pool, user_id)
            .await
            .expect("read notification preferences");
        assert!(preferences.is_none());
        let row_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_notification_preferences WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("count notification preference rows");
        assert_eq!(row_count, 0);
    }

    #[tokio::test]
    #[ignore = "requires migrated CRYSTAL_FORGE_TEST_DATABASE_URL"]
    async fn prohibited_email_materialization_is_not_retroactively_queued() {
        let pool = test_pool().await;
        let user_id = create_test_user(&pool, "email-policy-boundary").await;
        get_or_create_notification_preferences(&pool, user_id)
            .await
            .expect("initialize notification preferences");
        sqlx::query(
            "UPDATE user_notification_preferences
             SET delivery_channel = 'both',
                 build_failures_email_enabled_at = NOW(),
                 build_failures_in_app_enabled_at = NOW()
             WHERE user_id = $1",
        )
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("enable both notification channels");

        let first_occurrence_id = Uuid::new_v4();
        for (occurrence_id, key) in [
            (first_occurrence_id, "allowed"),
            (Uuid::new_v4(), "prohibited"),
        ] {
            sqlx::query(
                "INSERT INTO attention_occurrences
                    (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at)
                 VALUES ($1, 'builds', 'builds', $2, $3, NOW(), NOW())",
            )
            .bind(occurrence_id)
            .bind(Uuid::new_v4().to_string())
            .bind(format!("test-email-policy-{key}-{occurrence_id}"))
            .execute(&pool)
            .await
            .expect("insert attention occurrence");

            let permitted = key == "allowed";
            materialize_attention_notifications_for_user(&pool, user_id, permitted)
                .await
                .expect("materialize policy-boundary notification");
        }

        let delivery_count_before_reenable: (i64,) = sqlx::query_as(
            "SELECT COUNT(*)
             FROM user_notification_email_deliveries
             WHERE user_id = $1 AND delivery_type = 'immediate'",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("count deliveries before re-enable");
        assert_eq!(delivery_count_before_reenable.0, 1);

        materialize_attention_notifications_for_user(&pool, user_id, true)
            .await
            .expect("rematerialize after policy re-enable");

        let deliveries: Vec<(String,)> = sqlx::query_as(
            "SELECT idempotency_key
             FROM user_notification_email_deliveries
             WHERE user_id = $1 AND delivery_type = 'immediate'
             ORDER BY idempotency_key",
        )
        .bind(user_id)
        .fetch_all(&pool)
        .await
        .expect("list policy-boundary deliveries");
        assert_eq!(deliveries.len(), 1);
        assert!(deliveries[0].0.contains(&first_occurrence_id.to_string()));

        let notification_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM user_notifications
             WHERE user_id = $1 AND source_occurrence_id IS NOT NULL",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("count policy-boundary notifications");
        assert_eq!(notification_count.0, 2);
        sqlx::query(
            "UPDATE user_notification_email_deliveries
             SET state = 'cancelled'
             WHERE user_id = $1 AND delivery_type = 'immediate'",
        )
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("clean up immediate delivery");
    }

    #[tokio::test]
    #[ignore = "requires migrated CRYSTAL_FORGE_TEST_DATABASE_URL"]
    async fn weekly_digest_excludes_prohibited_notifications() {
        let pool = test_pool().await;
        let user_id = create_test_user(&pool, "digest-email-policy").await;
        let period_start = chrono::Utc::now() - chrono::Duration::days(7);
        let period_end = chrono::Utc::now();
        sqlx::query(
            "UPDATE user_notification_preferences
             SET weekly_digest = TRUE,
                 delivery_channel = 'email',
                 weekly_digest_enabled_at = $2,
                 build_failures_email_enabled_at = $2
             WHERE user_id = $1",
        )
        .bind(user_id)
        .bind(period_start)
        .execute(&pool)
        .await
        .expect("enable weekly digest");

        sqlx::query(
            "INSERT INTO user_notifications
                (user_id, category, source_type, source_id, title, summary, route,
                 email_delivery_eligible, created_at)
             VALUES ($1, 'build_failures', 'builds', $2, 'Suppressed', 'Suppressed', '/builds', FALSE, $3)",
        )
        .bind(user_id)
        .bind(Uuid::new_v4().to_string())
        .bind(period_start + chrono::Duration::hours(1))
        .execute(&pool)
        .await
        .expect("insert suppressed digest notification");

        assert!(
            !enqueue_weekly_digest_delivery(&pool, user_id, period_start, period_end)
                .await
                .expect("enqueue suppressed-only digest")
        );

        sqlx::query(
            "INSERT INTO user_notifications
                (user_id, category, source_type, source_id, title, summary, route,
                 email_delivery_eligible, created_at)
             VALUES ($1, 'build_failures', 'builds', $2, 'Allowed', 'Allowed', '/builds', TRUE, $3)",
        )
        .bind(user_id)
        .bind(Uuid::new_v4().to_string())
        .bind(period_start + chrono::Duration::hours(2))
        .execute(&pool)
        .await
        .expect("insert allowed digest notification");

        assert!(
            enqueue_weekly_digest_delivery(&pool, user_id, period_start, period_end)
                .await
                .expect("enqueue mixed digest")
        );
        let (delivery_count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM user_notification_email_deliveries
             WHERE user_id = $1 AND delivery_type = 'weekly_digest'",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("count digest deliveries");
        assert_eq!(delivery_count, 1);
        sqlx::query(
            "UPDATE user_notification_email_deliveries
             SET state = 'cancelled'
             WHERE user_id = $1 AND delivery_type = 'weekly_digest'",
        )
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("clean up digest delivery");
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
