use crate::api::models::UpdateNotificationPreferences;
use crate::models::user_notifications::{UserNotification, UserNotificationPreferences};
use sqlx::PgPool;
use uuid::Uuid;

const MATERIALIZATION_USERS_PER_PASS: i64 = 32;
const MATERIALIZATION_EVENTS_PER_USER: i64 = 8;

/// Materializes one bounded notification batch for an active user.
///
/// The producer uses the user's account creation time when it must initialize
/// missing preferences. A durable source-queue cursor bounds each pass before
/// authorization and deduplication, so skipped rows do not cause repeated
/// history scans. Existing notification rows preserve read and dismissed state.
///
/// # Errors
///
/// Returns a database error when preference initialization, notification
/// materialization, or immediate-email enqueueing fails.
pub async fn materialize_attention_notifications_for_user(
    pool: &PgPool,
    user_id: Uuid,
    email_delivery_permitted: bool,
) -> Result<u64, sqlx::Error> {
    materialize_user_notifications(pool, Some(user_id), email_delivery_permitted, 1, 256).await
}

/// Materializes one bounded notification batch for every active user.
///
/// This function uses a fixed number of set-oriented SQL statements. A durable
/// scheduler rotates across at most 32 users and consumes at most eight events
/// for each selected user. One process pass therefore examines at most 256
/// user/event pairs. Later passes provide fair, eventual progress.
///
/// # Errors
///
/// Returns a database error when preference initialization, notification
/// materialization, or immediate-email enqueueing fails.
pub async fn materialize_all_user_notifications(
    pool: &PgPool,
    email_delivery_permitted: bool,
) -> Result<u64, sqlx::Error> {
    materialize_user_notifications(
        pool,
        None,
        email_delivery_permitted,
        MATERIALIZATION_USERS_PER_PASS,
        MATERIALIZATION_EVENTS_PER_USER,
    )
    .await
}

async fn materialize_user_notifications(
    pool: &PgPool,
    user_id: Option<Uuid>,
    email_delivery_permitted: bool,
    users_per_pass: i64,
    events_per_user: i64,
) -> Result<u64, sqlx::Error> {
    let mut tx = pool.begin().await?;
    // Upgrade bootstrap is insert-only, bounded, and serialized with producers.
    // It does not mutate immutable history or lock source rows.
    let _: i64 = sqlx::query_scalar("SELECT backfill_user_notification_source_events(256)")
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query(
        r#"INSERT INTO user_notification_preferences (
               user_id, deploy_failures_in_app_enabled_at,
               build_failures_in_app_enabled_at,
               critical_cves_in_app_enabled_at,
               policy_violations_in_app_enabled_at, initialized_at
           )
           SELECT u.id, u.created_at, u.created_at, u.created_at, u.created_at, u.created_at
           FROM users u
           WHERE u.is_active = TRUE
             AND ($1::uuid IS NULL OR u.id = $1)
           ON CONFLICT (user_id) DO NOTHING"#,
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        r#"INSERT INTO user_notification_materialization_schedule(user_id)
           SELECT p.user_id
           FROM user_notification_preferences p
           JOIN users u ON u.id = p.user_id
           WHERE u.is_active = TRUE
             AND ($1::uuid IS NULL OR p.user_id = $1)
           ON CONFLICT (user_id) DO NOTHING"#,
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        r#"INSERT INTO user_notification_source_cursors(user_id, last_event_id)
           SELECT p.user_id, COALESCE((
               SELECT MAX(event.id)
               FROM user_notification_source_events event
               WHERE event.occurred_at < p.initialized_at
           ), 0)
           FROM user_notification_preferences p
           JOIN users u ON u.id = p.user_id
           WHERE u.is_active = TRUE
             AND ($1::uuid IS NULL OR p.user_id = $1)
           ON CONFLICT (user_id) DO NOTHING"#,
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    // CONCURRENCY: Producers serialize queue identity allocation through commit.
    // This query locks each selected scheduler row and cursor, so overlapping
    // passes cannot claim the same user or independently advance its high-water
    // mark. Unique inbox indexes remain the final deduplication backstop.
    let inserted: i64 = sqlx::query_scalar(
        r#"
        WITH prefs AS (
            SELECT p.*
            FROM user_notification_preferences p
            JOIN users u ON u.id = p.user_id
            WHERE u.is_active = TRUE
              AND ($1::uuid IS NULL OR p.user_id = $1)
        ), cursors AS MATERIALIZED (
            SELECT cursor.user_id, cursor.last_event_id
            FROM user_notification_materialization_schedule schedule
            JOIN user_notification_source_cursors cursor ON cursor.user_id = schedule.user_id
            JOIN prefs ON prefs.user_id = cursor.user_id
            ORDER BY schedule.last_serviced_at, schedule.user_id
            LIMIT $3
            FOR UPDATE OF schedule, cursor SKIP LOCKED
        ), scanned AS MATERIALIZED (
            SELECT cursor.user_id, source_event.id AS event_id,
                   source_event.source_kind, source_event.source_id
            FROM cursors cursor
            CROSS JOIN LATERAL (
                SELECT event.id, event.source_kind, event.source_id
                FROM user_notification_source_events event
                WHERE event.id > cursor.last_event_id
                ORDER BY event.id
                LIMIT $4
            ) source_event
        ), sources AS MATERIALIZED (
            SELECT scanned.user_id, scanned.event_id, event.category,
                   event.source_occurrence_id,
                   event.notification_source_type AS source_type,
                   event.notification_source_id AS source_id,
                   event.occurred_at AS opened_at, event.title, event.summary,
                   event.route, event.authorization_scope,
                   event.authorization_environment_ids
            FROM scanned
            JOIN user_notification_source_events event ON event.id = scanned.event_id
        ), candidates AS MATERIALIZED (
            SELECT source.*,
                COALESCE(
                    p.delivery_channel IN ('in_app', 'both')
                    AND source.opened_at >= CASE source.category
                        WHEN 'deploy_failures' THEN p.deploy_failures_in_app_enabled_at
                        WHEN 'build_failures' THEN p.build_failures_in_app_enabled_at
                        WHEN 'critical_cves' THEN p.critical_cves_in_app_enabled_at
                        WHEN 'policy_violations' THEN p.policy_violations_in_app_enabled_at
                        WHEN 'heartbeat_lost' THEN p.heartbeat_lost_in_app_enabled_at
                    END,
                    FALSE
                ) AS in_app_visible,
                COALESCE(
                    $2 AND p.delivery_channel IN ('email', 'both')
                    AND source.opened_at >= CASE source.category
                        WHEN 'deploy_failures' THEN p.deploy_failures_email_enabled_at
                        WHEN 'build_failures' THEN p.build_failures_email_enabled_at
                        WHEN 'critical_cves' THEN p.critical_cves_email_enabled_at
                        WHEN 'policy_violations' THEN p.policy_violations_email_enabled_at
                        WHEN 'heartbeat_lost' THEN p.heartbeat_lost_email_enabled_at
                    END,
                    FALSE
                ) AS email_delivery_eligible
            FROM prefs p
            JOIN sources source ON source.user_id = p.user_id
            WHERE (
                    (source.category = 'deploy_failures' AND p.deploy_failures)
                 OR (source.category = 'build_failures' AND p.build_failures)
                 OR (source.category = 'critical_cves' AND p.critical_cves)
                 OR (source.category = 'policy_violations' AND p.policy_violations)
                 OR (source.category = 'heartbeat_lost' AND p.heartbeat_lost)
            )
              AND (
                    COALESCE(p.delivery_channel IN ('in_app', 'both') AND source.opened_at >= CASE source.category
                        WHEN 'deploy_failures' THEN p.deploy_failures_in_app_enabled_at
                        WHEN 'build_failures' THEN p.build_failures_in_app_enabled_at
                        WHEN 'critical_cves' THEN p.critical_cves_in_app_enabled_at
                        WHEN 'policy_violations' THEN p.policy_violations_in_app_enabled_at
                        WHEN 'heartbeat_lost' THEN p.heartbeat_lost_in_app_enabled_at
                    END, FALSE)
                 OR COALESCE($2 AND p.delivery_channel IN ('email', 'both') AND source.opened_at >= CASE source.category
                        WHEN 'deploy_failures' THEN p.deploy_failures_email_enabled_at
                        WHEN 'build_failures' THEN p.build_failures_email_enabled_at
                        WHEN 'critical_cves' THEN p.critical_cves_email_enabled_at
                        WHEN 'policy_violations' THEN p.policy_violations_email_enabled_at
                        WHEN 'heartbeat_lost' THEN p.heartbeat_lost_email_enabled_at
                    END, FALSE)
              )
              AND notification_visible_to_user_snapshot(
                    p.user_id, source.source_type, source.source_id,
                    source.authorization_scope, source.authorization_environment_ids
              )
              AND NOT EXISTS (
                  SELECT 1 FROM user_notifications existing
                  WHERE existing.user_id = p.user_id
                    AND existing.category = source.category
                    AND (
                          (source.source_occurrence_id IS NOT NULL
                           AND existing.source_occurrence_id = source.source_occurrence_id)
                       OR (source.source_occurrence_id IS NULL
                           AND existing.source_type = source.source_type
                           AND existing.source_id = source.source_id)
                    )
              )
        ), notifications AS (
            INSERT INTO user_notifications (
                user_id, category, source_occurrence_id, source_type, source_id,
                title, summary, route, in_app_visible, email_delivery_eligible, created_at,
                authorization_scope, authorization_environment_ids
            )
            SELECT user_id, category, source_occurrence_id, source_type, source_id,
                   title, summary, route, in_app_visible, email_delivery_eligible, opened_at,
                   authorization_scope, authorization_environment_ids
            FROM candidates
            ON CONFLICT DO NOTHING
            RETURNING 1
        ), advanced AS (
            UPDATE user_notification_source_cursors cursor
            SET last_event_id = maximum.event_id, updated_at = CURRENT_TIMESTAMP
            FROM (
                SELECT user_id, MAX(event_id) AS event_id
                FROM scanned GROUP BY user_id
            ) maximum
            WHERE cursor.user_id = maximum.user_id
            RETURNING 1
        ), serviced AS (
            UPDATE user_notification_materialization_schedule schedule
            SET last_serviced_at = clock_timestamp()
            FROM cursors
            WHERE schedule.user_id = cursors.user_id
            RETURNING 1
        )
        SELECT (SELECT COUNT(*) FROM notifications)
             + 0 * (SELECT COUNT(*) FROM advanced)
             + 0 * (SELECT COUNT(*) FROM serviced)
        "#,
    )
    .bind(user_id)
    .bind(email_delivery_permitted)
    .bind(users_per_pass.clamp(1, MATERIALIZATION_USERS_PER_PASS))
    .bind(events_per_user.clamp(1, 256))
    .fetch_one(&mut *tx)
    .await?;

    if !email_delivery_permitted {
        tx.commit().await?;
        return Ok(inserted as u64);
    }

    sqlx::query(
        r#"INSERT INTO user_notification_immediate_email_cursors(user_id)
           SELECT p.user_id
           FROM user_notification_preferences p
           JOIN users u ON u.id = p.user_id
           WHERE u.is_active = TRUE
             AND p.delivery_channel IN ('email', 'both')
             AND ($1::uuid IS NULL OR p.user_id = $1)
           ON CONFLICT (user_id) DO NOTHING"#,
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    // Email candidates come only from durable inbox rows. This preserves the
    // eligibility decision made while external delivery was permitted and
    // avoids a second scan of source-event history.
    let _: i64 = sqlx::query_scalar(
        r#"
        WITH prefs AS (
            SELECT p.*
            FROM user_notification_preferences p
            JOIN users u ON u.id = p.user_id
            WHERE u.is_active = TRUE
              AND ($1::uuid IS NULL OR p.user_id = $1)
              AND p.delivery_channel IN ('email', 'both')
        ), cursors AS MATERIALIZED (
            SELECT cursor.user_id, cursor.last_materialization_order
            FROM user_notification_immediate_email_cursors cursor
            JOIN prefs ON prefs.user_id = cursor.user_id
            ORDER BY cursor.updated_at, cursor.user_id
            LIMIT $3
            FOR UPDATE OF cursor
        ), scanned AS MATERIALIZED (
            SELECT cursor.user_id, candidate.*
            FROM cursors cursor
            CROSS JOIN LATERAL (
                SELECT n.id, n.materialization_order, n.category,
                       n.source_occurrence_id, n.source_type, n.source_id,
                       n.created_at, n.email_delivery_eligible,
                       n.authorization_scope, n.authorization_environment_ids
                FROM user_notifications n
                WHERE n.user_id = cursor.user_id
                  AND n.materialization_order > cursor.last_materialization_order
                ORDER BY n.materialization_order
                LIMIT $2
            ) candidate
        ), candidates AS MATERIALIZED (
            SELECT scanned.*
            FROM prefs p
            JOIN scanned ON scanned.user_id = p.user_id
            WHERE scanned.email_delivery_eligible
                  AND NOT EXISTS (
                      SELECT 1
                      FROM user_notification_email_deliveries delivery
                      WHERE delivery.notification_id = scanned.id
                        AND delivery.delivery_type = 'immediate'
                  )
                  AND (
                        (scanned.category = 'deploy_failures' AND p.deploy_failures
                         AND scanned.created_at >= p.deploy_failures_email_enabled_at)
                     OR (scanned.category = 'build_failures' AND p.build_failures
                         AND scanned.created_at >= p.build_failures_email_enabled_at)
                     OR (scanned.category = 'critical_cves' AND p.critical_cves
                         AND scanned.created_at >= p.critical_cves_email_enabled_at)
                     OR (scanned.category = 'policy_violations' AND p.policy_violations
                         AND scanned.created_at >= p.policy_violations_email_enabled_at)
                     OR (scanned.category = 'heartbeat_lost' AND p.heartbeat_lost
                         AND scanned.created_at >= p.heartbeat_lost_email_enabled_at)
                   )
                   AND notification_visible_to_user_snapshot(
                        p.user_id, scanned.source_type, scanned.source_id,
                        scanned.authorization_scope,
                        scanned.authorization_environment_ids
                   )
        ), deliveries AS (
          INSERT INTO user_notification_email_deliveries (
            user_id, notification_id, delivery_type, idempotency_key
        )
        SELECT
            user_id,
            id,
            'immediate',
            'immediate:' || user_id::text || ':' || CASE
                WHEN source_occurrence_id IS NOT NULL
                    THEN 'source_occurrence:' || source_occurrence_id::text
                ELSE source_type || ':' || source_id
            END
        FROM candidates
          ON CONFLICT (idempotency_key) DO NOTHING
          RETURNING 1
        ), advanced AS (
          UPDATE user_notification_immediate_email_cursors cursor
          SET last_materialization_order = maximum.materialization_order,
              updated_at = CURRENT_TIMESTAMP
          FROM (
              SELECT user_id, MAX(materialization_order) AS materialization_order
              FROM scanned GROUP BY user_id
          ) maximum
          WHERE cursor.user_id = maximum.user_id
          RETURNING 1
        )
        SELECT (SELECT COUNT(*) FROM deliveries)
             + 0 * (SELECT COUNT(*) FROM advanced)
        "#,
    )
    .bind(user_id)
    .bind(events_per_user.clamp(1, 256))
    .bind(users_per_pass.clamp(1, MATERIALIZATION_USERS_PER_PASS))
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(inserted as u64)
}

/// Enqueues due weekly digests for all eligible active users in one query.
///
/// # Errors
///
/// Returns a database error when the period or digest enqueue query fails.
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
    let result = sqlx::query(
        r#"
        WITH eligible AS (
            SELECT p.user_id
            FROM user_notification_preferences p
            JOIN users u ON u.id = p.user_id
            WHERE u.is_active = TRUE
              AND p.weekly_digest = TRUE
              AND p.delivery_channel IN ('email', 'both')
              AND p.weekly_digest_enabled_at IS NOT NULL
              AND p.weekly_digest_enabled_at < $2
              AND EXISTS (
                  SELECT 1
                  FROM user_notifications n
                  WHERE n.user_id = p.user_id
                    AND n.email_delivery_eligible
                    AND n.dismissed_at IS NULL
                    AND n.created_at >= GREATEST($1, p.weekly_digest_enabled_at)
                    AND n.created_at < $2
                    AND notification_visible_to_user_snapshot(
                        p.user_id, n.source_type, n.source_id,
                        n.authorization_scope, n.authorization_environment_ids
                    )
                    AND NOT EXISTS (
                        SELECT 1
                        FROM user_notification_source_cursors cursor
                        WHERE cursor.user_id = p.user_id
                          AND EXISTS (
                              SELECT 1 FROM user_notification_source_events event
                              WHERE event.id > cursor.last_event_id
                          )
                    )
              )
        ), delivery AS (
            INSERT INTO user_notification_email_deliveries (
                user_id, delivery_type, idempotency_key
            )
            SELECT
                user_id,
                'weekly_digest',
                'weekly_digest:' || user_id::text || ':' || $1::text || ':' || $2::text
            FROM eligible
            ON CONFLICT (idempotency_key) DO NOTHING
            RETURNING id, user_id
        )
        INSERT INTO user_notification_weekly_digest_runs (
            user_id, period_start, period_end, status, delivery_id
        )
        SELECT user_id, $1, $2, 'pending', id
        FROM delivery
        ON CONFLICT (user_id, period_start, period_end) DO NOTHING
        "#,
    )
    .bind(period_start)
    .bind(period_end)
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
              AND notification_visible_to_user_snapshot(
                    $1, source_type, source_id,
                    authorization_scope, authorization_environment_ids
              )
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
           AND notification_visible_to_user_snapshot(
                $1, source_type, source_id,
                authorization_scope, authorization_environment_ids
           )
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
           AND notification_visible_to_user_snapshot(
                $1, source_type, source_id,
                authorization_scope, authorization_environment_ids
           )",
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
           AND notification_visible_to_user_snapshot(
                $2, source_type, source_id,
                authorization_scope, authorization_environment_ids
           )",
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
           AND notification_visible_to_user_snapshot(
                $1, source_type, source_id,
                authorization_scope, authorization_environment_ids
           )",
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
           AND notification_visible_to_user_snapshot(
                $2, source_type, source_id,
                authorization_scope, authorization_environment_ids
           )",
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
        assert!(
            rows.iter()
                .any(|row| row.source_occurrence_id == Some(occurrence_id))
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires repository-provided isolated PostgreSQL"]
    async fn notification_materialization_bounds_each_users_missing_history(pool: PgPool) {
        let user_id = create_test_user(&pool, "bounded-history").await;
        let fixture_key = Uuid::new_v4().to_string();
        let fixture_prefix = format!("bounded-history-{fixture_key}-");
        let candidate_limit = 7;
        let expected_count = 19;

        sqlx::query(
            "INSERT INTO attention_occurrences
                (id, category, subject_type, subject_id, source_occurrence_key,
                 opened_at, last_observed_at)
             SELECT gen_random_uuid(), 'builds', 'builds', $1 || series::text,
                    $2 || series::text, NOW(), NOW()
             FROM generate_series(1, $3::integer) series",
        )
        .bind(&fixture_prefix)
        .bind(format!("bounded-history-key-{fixture_key}-"))
        .bind(expected_count)
        .execute(&pool)
        .await
        .expect("insert bounded notification history");

        let first_count =
            materialize_user_notifications(&pool, Some(user_id), false, 1, candidate_limit)
                .await
                .expect("materialize first bounded batch");
        assert!(first_count > 0);
        assert!(first_count <= candidate_limit as u64);

        for _ in 0..20 {
            let materialized: i64 = sqlx::query_scalar(
                "SELECT COUNT(*)
                 FROM user_notifications
                 WHERE user_id = $1 AND source_id LIKE $2",
            )
            .bind(user_id)
            .bind(format!("{fixture_prefix}%"))
            .fetch_one(&pool)
            .await
            .expect("count bounded notification history");
            if materialized == i64::from(expected_count) {
                break;
            }

            let pass_count =
                materialize_user_notifications(&pool, Some(user_id), false, 1, candidate_limit)
                    .await
                    .expect("drain bounded notification history");
            assert!(pass_count <= candidate_limit as u64);
        }

        let materialized: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM user_notifications
             WHERE user_id = $1 AND source_id LIKE $2",
        )
        .bind(user_id)
        .bind(format!("{fixture_prefix}%"))
        .fetch_one(&pool)
        .await
        .expect("count final bounded notification history");
        assert_eq!(materialized, i64::from(expected_count));
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "runs in the PostgreSQL server-regressions check"]
    async fn notification_queue_serializes_identity_allocation_through_commit(pool: PgPool) {
        let user_id = create_test_user(&pool, "commit-order").await;
        materialize_attention_notifications_for_user(&pool, user_id, false)
            .await
            .expect("initialize notification cursor");
        let first_id = Uuid::new_v4();
        let second_id = Uuid::new_v4();
        let mut first = pool.begin().await.expect("begin first source transaction");
        sqlx::query(
            "INSERT INTO attention_occurrences
                (id, category, subject_type, subject_id, source_occurrence_key,
                 opened_at, last_observed_at)
             VALUES ($1, 'builds', 'builds', $2, $3, NOW(), NOW())",
        )
        .bind(first_id)
        .bind(Uuid::new_v4().to_string())
        .bind(format!("commit-order-first-{first_id}"))
        .execute(&mut *first)
        .await
        .expect("insert uncommitted first source");

        let writer_pool = pool.clone();
        let second = tokio::spawn(async move {
            sqlx::query(
                "INSERT INTO attention_occurrences
                    (id, category, subject_type, subject_id, source_occurrence_key,
                     opened_at, last_observed_at)
                 VALUES ($1, 'builds', 'builds', $2, $3, NOW(), NOW())",
            )
            .bind(second_id)
            .bind(Uuid::new_v4().to_string())
            .bind(format!("commit-order-second-{second_id}"))
            .execute(&writer_pool)
            .await
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(
            !second.is_finished(),
            "later queue identity allocation must wait for the earlier transaction"
        );
        first.commit().await.expect("commit first source");
        second
            .await
            .expect("join second source writer")
            .expect("insert second source");

        materialize_attention_notifications_for_user(&pool, user_id, false)
            .await
            .expect("materialize commit-ordered sources");
        let delivered: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_notifications
             WHERE user_id=$1 AND source_occurrence_id=ANY($2)",
        )
        .bind(user_id)
        .bind(vec![first_id, second_id])
        .fetch_one(&pool)
        .await
        .expect("count commit-ordered notifications");
        assert_eq!(delivered, 2);
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "runs in the PostgreSQL server-regressions check"]
    async fn notification_queue_snapshot_survives_source_deletion(pool: PgPool) {
        let user_id = create_test_user(&pool, "source-deletion").await;
        materialize_attention_notifications_for_user(&pool, user_id, false)
            .await
            .expect("initialize notification cursor");
        let occurrence_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO attention_occurrences
                (id, category, subject_type, subject_id, source_occurrence_key,
                 opened_at, last_observed_at)
             VALUES ($1, 'builds', 'builds', $2, $3, NOW(), NOW())",
        )
        .bind(occurrence_id)
        .bind(Uuid::new_v4().to_string())
        .bind(format!("deleted-source-{occurrence_id}"))
        .execute(&pool)
        .await
        .expect("insert deletable source");
        sqlx::query("DELETE FROM attention_occurrences WHERE id=$1")
            .bind(occurrence_id)
            .execute(&pool)
            .await
            .expect("delete source after enqueue");

        materialize_attention_notifications_for_user(&pool, user_id, false)
            .await
            .expect("materialize deleted source snapshot");
        let notifications = list_notifications(&pool, user_id, 20, None, false)
            .await
            .expect("list notification after source deletion");
        assert!(
            notifications
                .iter()
                .any(|notification| notification.source_occurrence_id == Some(occurrence_id))
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "runs in the PostgreSQL server-regressions check"]
    async fn notification_materialization_has_global_bound_and_fair_progress(pool: PgPool) {
        let mut user_ids = Vec::new();
        for index in 0..40 {
            user_ids.push(create_test_user(&pool, &format!("global-bound-{index}")).await);
        }
        let fixture = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO attention_occurrences
                (id, category, subject_type, subject_id, source_occurrence_key,
                 opened_at, last_observed_at)
             SELECT gen_random_uuid(), 'builds', 'builds', gen_random_uuid()::text,
                    $1 || series::text, NOW(), NOW()
             FROM generate_series(1, 20) series",
        )
        .bind(format!("global-bound-{fixture}-"))
        .execute(&pool)
        .await
        .expect("insert global-bound sources");

        let first = materialize_all_user_notifications(&pool, false)
            .await
            .expect("materialize first global batch");
        assert_eq!(first, 256);
        let (notifications, users): (i64, i64) =
            sqlx::query_as("SELECT COUNT(*),COUNT(DISTINCT user_id) FROM user_notifications")
                .fetch_one(&pool)
                .await
                .expect("count first global batch");
        assert_eq!((notifications, users), (256, 32));

        materialize_all_user_notifications(&pool, false)
            .await
            .expect("rotate to unserviced users");
        let users_after_rotation: i64 =
            sqlx::query_scalar("SELECT COUNT(DISTINCT user_id) FROM user_notifications")
                .fetch_one(&pool)
                .await
                .expect("count users after fair rotation");
        assert_eq!(users_after_rotation, 40);

        for _ in 0..4 {
            materialize_all_user_notifications(&pool, false)
                .await
                .expect("drain global notification backlog");
        }
        let final_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_notifications")
            .fetch_one(&pool)
            .await
            .expect("count drained global notifications");
        assert_eq!(final_count, 800);
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires repository-provided isolated PostgreSQL"]
    async fn notification_cursor_advances_past_skipped_source_rows(pool: PgPool) {
        let user_id = create_test_user(&pool, "skipped-source-cursor").await;
        let skipped_occurrence_id = Uuid::new_v4();
        let delivered_occurrence_id = Uuid::new_v4();

        sqlx::query(
            "INSERT INTO attention_occurrences
                (id, category, subject_type, subject_id, source_occurrence_key,
                 opened_at, last_observed_at)
             VALUES ($1, 'builds', 'builds', $2, $3, NOW(), NOW())",
        )
        .bind(skipped_occurrence_id)
        .bind(Uuid::new_v4().to_string())
        .bind(format!("already-materialized-{skipped_occurrence_id}"))
        .execute(&pool)
        .await
        .expect("insert already-materialized source");
        sqlx::query(
            "INSERT INTO user_notifications
                (user_id, category, source_occurrence_id, source_type, source_id,
                 title, summary, route, created_at)
             SELECT $1, 'build_failures', id, 'builds', subject_id,
                    'Existing', 'Existing', '/builds', opened_at
             FROM attention_occurrences WHERE id = $2",
        )
        .bind(user_id)
        .bind(skipped_occurrence_id)
        .execute(&pool)
        .await
        .expect("insert existing notification");

        sqlx::query(
            "INSERT INTO attention_occurrences
                (id, category, subject_type, subject_id, source_occurrence_key,
                 opened_at, last_observed_at)
             SELECT gen_random_uuid(), 'poams', 'poams', gen_random_uuid()::text,
                    $1 || series::text, NOW(), NOW()
             FROM generate_series(1, 8) series",
        )
        .bind(format!("unauthorized-source-{}-", Uuid::new_v4()))
        .execute(&pool)
        .await
        .expect("insert unauthorized sources");
        sqlx::query(
            "INSERT INTO attention_occurrences
                (id, category, subject_type, subject_id, source_occurrence_key,
                 opened_at, last_observed_at)
             VALUES ($1, 'builds', 'builds', $2, $3, NOW(), NOW())",
        )
        .bind(delivered_occurrence_id)
        .bind(Uuid::new_v4().to_string())
        .bind(format!("delivered-after-skips-{delivered_occurrence_id}"))
        .execute(&pool)
        .await
        .expect("insert deliverable source");

        for _ in 0..5 {
            materialize_user_notifications(&pool, Some(user_id), false, 1, 4)
                .await
                .expect("advance bounded source cursor");
            let delivered: bool = sqlx::query_scalar(
                "SELECT EXISTS (
                     SELECT 1 FROM user_notifications
                     WHERE user_id = $1 AND source_occurrence_id = $2
                 )",
            )
            .bind(user_id)
            .bind(delivered_occurrence_id)
            .fetch_one(&pool)
            .await
            .expect("check bounded-pass delivery");
            if delivered {
                break;
            }
        }

        let delivered: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM user_notifications
                 WHERE user_id = $1 AND source_occurrence_id = $2
             )",
        )
        .bind(user_id)
        .bind(delivered_occurrence_id)
        .fetch_one(&pool)
        .await
        .expect("check eventual delivery");
        assert!(delivered);

        let duplicate_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_notifications
             WHERE user_id = $1 AND source_occurrence_id = $2",
        )
        .bind(user_id)
        .bind(skipped_occurrence_id)
        .fetch_one(&pool)
        .await
        .expect("count existing notification");
        assert_eq!(duplicate_count, 1);
    }

    #[tokio::test]
    #[ignore = "requires migrated CRYSTAL_FORGE_TEST_DATABASE_URL"]
    async fn account_reactivation_excludes_inactive_period_notifications_and_email() {
        let pool = test_pool().await;
        let user_id = create_test_user(&pool, "reactivation-boundary").await;
        sqlx::query(
            "UPDATE user_notification_preferences
             SET delivery_channel = 'both',
                 build_failures_email_enabled_at = NOW() - INTERVAL '1 day',
                 build_failures_in_app_enabled_at = NOW() - INTERVAL '1 day'
             WHERE user_id = $1",
        )
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("enable both notification channels");
        crate::queries::admin::update_user_active(&pool, user_id, false)
            .await
            .expect("deactivate notification user");

        let inactive_occurrence_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO attention_occurrences
                (id, category, subject_type, subject_id, source_occurrence_key,
                 opened_at, last_observed_at)
             VALUES ($1, 'builds', 'builds', $2, $3,
                     NOW() - INTERVAL '1 second', NOW() - INTERVAL '1 second')",
        )
        .bind(inactive_occurrence_id)
        .bind(Uuid::new_v4().to_string())
        .bind(format!("inactive-account-{inactive_occurrence_id}"))
        .execute(&pool)
        .await
        .expect("insert inactive-period occurrence");

        crate::queries::admin::update_user_active(&pool, user_id, true)
            .await
            .expect("reactivate notification user");
        let active_occurrence_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO attention_occurrences
                (id, category, subject_type, subject_id, source_occurrence_key,
                 opened_at, last_observed_at)
             VALUES ($1, 'builds', 'builds', $2, $3,
                     NOW() + INTERVAL '1 second', NOW() + INTERVAL '1 second')",
        )
        .bind(active_occurrence_id)
        .bind(Uuid::new_v4().to_string())
        .bind(format!("reactivated-account-{active_occurrence_id}"))
        .execute(&pool)
        .await
        .expect("insert post-reactivation occurrence");

        materialize_attention_notifications_for_user(&pool, user_id, true)
            .await
            .expect("materialize post-reactivation notifications");
        let materialized: Vec<(Uuid, bool)> = sqlx::query_as(
            "SELECT n.source_occurrence_id, EXISTS (
                 SELECT 1 FROM user_notification_email_deliveries d
                 WHERE d.notification_id = n.id AND d.delivery_type = 'immediate'
             )
             FROM user_notifications n
             WHERE n.user_id = $1 AND n.source_occurrence_id = ANY($2)",
        )
        .bind(user_id)
        .bind(vec![inactive_occurrence_id, active_occurrence_id])
        .fetch_all(&pool)
        .await
        .expect("read reactivation notification state");
        assert_eq!(materialized, vec![(active_occurrence_id, true)]);
        sqlx::query(
            "UPDATE user_notification_email_deliveries
             SET state = 'cancelled'
             WHERE user_id = $1 AND state = 'pending'",
        )
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("clean up reactivation delivery");
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
             SET delivery_channel = 'email',
                 build_failures_email_enabled_at = NOW(),
                 build_failures_in_app_enabled_at = NULL
             WHERE user_id = $1",
        )
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("enable email-only notifications");

        let first_occurrence_id = Uuid::new_v4();
        let prohibited_occurrence_id = Uuid::new_v4();
        for (occurrence_id, key) in [
            (first_occurrence_id, "allowed"),
            (prohibited_occurrence_id, "prohibited"),
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
             WHERE user_id = $1 AND delivery_type = 'immediate'
               AND notification_id IN (
                   SELECT id FROM user_notifications
                   WHERE user_id = $1 AND source_occurrence_id = ANY($2)
               )",
        )
        .bind(user_id)
        .bind(vec![first_occurrence_id, prohibited_occurrence_id])
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
               AND notification_id IN (
                   SELECT id FROM user_notifications
                   WHERE user_id = $1 AND source_occurrence_id = ANY($2)
               )
             ORDER BY idempotency_key",
        )
        .bind(user_id)
        .bind(vec![first_occurrence_id, prohibited_occurrence_id])
        .fetch_all(&pool)
        .await
        .expect("list policy-boundary deliveries");
        assert_eq!(deliveries.len(), 1);
        assert!(deliveries[0].0.contains(&first_occurrence_id.to_string()));

        let notification_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM user_notifications
             WHERE user_id = $1 AND source_occurrence_id = ANY($2)",
        )
        .bind(user_id)
        .bind(vec![first_occurrence_id, prohibited_occurrence_id])
        .fetch_one(&pool)
        .await
        .expect("count policy-boundary notifications");
        assert_eq!(notification_count.0, 1);
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
    async fn weekly_digest_coordinator_enqueues_users_set_wise_and_idempotently() {
        let pool = test_pool().await;
        let user_id = create_test_user(&pool, "set-wise-digest").await;
        sqlx::query(
            "UPDATE user_notification_preferences
             SET weekly_digest = TRUE,
                 delivery_channel = 'email',
                 weekly_digest_enabled_at = date_trunc('week', NOW()) - INTERVAL '14 days',
                 build_failures_email_enabled_at = date_trunc('week', NOW()) - INTERVAL '14 days'
             WHERE user_id = $1",
        )
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("enable set-wise weekly digest");
        sqlx::query(
            "INSERT INTO user_notifications
                (user_id, category, source_type, source_id, title, summary, route,
                 email_delivery_eligible, created_at)
             VALUES ($1, 'build_failures', 'builds', $2, 'Digest candidate',
                     'Digest candidate', '/builds', TRUE,
                     date_trunc('week', NOW()) - INTERVAL '1 day')",
        )
        .bind(user_id)
        .bind(Uuid::new_v4().to_string())
        .execute(&pool)
        .await
        .expect("insert set-wise digest candidate");

        let first = enqueue_due_weekly_digest_deliveries(&pool, "weekly_utc")
            .await
            .expect("enqueue set-wise weekly digest");
        assert!(first >= 1);
        enqueue_due_weekly_digest_deliveries(&pool, "weekly_utc")
            .await
            .expect("repeat set-wise weekly digest enqueue");

        let run_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM user_notification_weekly_digest_runs
             WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("count set-wise digest runs");
        assert_eq!(run_count, 1);
        sqlx::query(
            "UPDATE user_notification_email_deliveries
             SET state = 'cancelled'
             WHERE user_id = $1 AND state = 'pending'",
        )
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("clean up set-wise digest delivery");
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

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires repository-provided isolated PostgreSQL"]
    async fn historical_poam_activity_bootstrap_is_insert_only_and_idempotent(pool: PgPool) {
        let user_id = create_test_user(&pool, "historical-poam-bootstrap").await;
        let poam_id = create_poam_fixture(&pool, user_id).await;
        let activity_id: Uuid = sqlx::query_scalar(
            "INSERT INTO poam_activity(poam_id,actor_user_id,kind,payload)
             VALUES($1,$2,'status_changed','{\"from\":\"in_progress\",\"to\":\"awaiting_verification\"}')
             RETURNING id",
        )
        .bind(poam_id)
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("insert historical POA&M activity");
        sqlx::query(
            "DELETE FROM user_notification_source_events
             WHERE source_kind='poam_activity' AND source_id=$1",
        )
        .bind(activity_id)
        .execute(&pool)
        .await
        .expect("simulate source history predating queue migration");

        materialize_attention_notifications_for_user(&pool, user_id, false)
            .await
            .expect("bootstrap historical POA&M activity");
        materialize_attention_notifications_for_user(&pool, user_id, false)
            .await
            .expect("retry historical POA&M bootstrap");

        let (activity_count, queue_count): (i64, i64) = sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM poam_activity WHERE id=$1),
                    (SELECT COUNT(*) FROM user_notification_source_events
                     WHERE source_kind='poam_activity' AND source_id=$1)",
        )
        .bind(activity_id)
        .fetch_one(&pool)
        .await
        .expect("read historical bootstrap result");
        assert_eq!((activity_count, queue_count), (1, 1));
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires repository-provided isolated PostgreSQL"]
    async fn cleanup_waits_for_historical_source_bootstrap_completion(pool: PgPool) {
        let occurrence_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO attention_occurrences(
                 id,category,subject_type,subject_id,source_occurrence_key,
                 opened_at,last_observed_at,resolved_at)
             VALUES($1,'builds','build_job',$2,$3,NOW()-INTERVAL '40 days',
                    NOW()-INTERVAL '40 days',NOW()-INTERVAL '39 days')",
        )
        .bind(occurrence_id)
        .bind(Uuid::new_v4().to_string())
        .bind(format!("historical-cleanup-{occurrence_id}"))
        .execute(&pool)
        .await
        .expect("insert old resolved occurrence");
        sqlx::query(
            "DELETE FROM user_notification_source_events
             WHERE source_kind='attention_occurrence' AND source_id=$1",
        )
        .bind(occurrence_id)
        .execute(&pool)
        .await
        .expect("simulate an unqueued pre-upgrade occurrence");

        let before_bootstrap =
            crate::queries::attention::cleanup(&pool, chrono::Duration::days(30), 1000)
                .await
                .expect("run cleanup before bootstrap");
        assert_eq!(before_bootstrap, (0, 0));
        let preserved: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attention_occurrences WHERE id=$1)")
                .bind(occurrence_id)
                .fetch_one(&pool)
                .await
                .expect("check preserved historical occurrence");
        assert!(preserved);

        let inserted: i64 =
            sqlx::query_scalar("SELECT backfill_user_notification_source_events(256)")
                .fetch_one(&pool)
                .await
                .expect("bootstrap historical source queue");
        assert_eq!(inserted, 1);
        let completed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM user_notification_source_bootstrap_state)",
        )
        .fetch_one(&pool)
        .await
        .expect("check durable bootstrap completion");
        assert!(completed);

        let after_bootstrap =
            crate::queries::attention::cleanup(&pool, chrono::Duration::days(30), 1000)
                .await
                .expect("run cleanup after bootstrap");
        assert_eq!(after_bootstrap.0, 1);
        let (source_deleted, queue_preserved): (bool, bool) = sqlx::query_as(
            "SELECT NOT EXISTS(SELECT 1 FROM attention_occurrences WHERE id=$1),
                    EXISTS(SELECT 1 FROM user_notification_source_events
                      WHERE source_kind='attention_occurrence' AND source_id=$1)",
        )
        .bind(occurrence_id)
        .fetch_one(&pool)
        .await
        .expect("check cleanup after source bootstrap");
        assert!(source_deleted);
        assert!(queue_preserved);
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires repository-provided isolated PostgreSQL"]
    async fn poam_notification_visibility_requires_every_current_finding_context(pool: PgPool) {
        let user_id = create_test_user(&pool, "partial-poam-context").await;
        let poam_id = create_poam_fixture(&pool, user_id).await;
        sqlx::query("UPDATE user_role_assignments SET role='viewer' WHERE user_id=$1")
            .bind(user_id)
            .execute(&pool)
            .await
            .expect("make fixture user a viewer");
        let visible_environment: Uuid = sqlx::query_scalar(
            "SELECT system.environment_id FROM poam_context_systems context
             JOIN systems system ON system.id=context.system_id WHERE context.poam_id=$1 LIMIT 1",
        )
        .bind(poam_id)
        .fetch_one(&pool)
        .await
        .expect("read visible environment");
        sqlx::query(
            "INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)",
        )
        .bind(user_id)
        .bind(visible_environment)
        .execute(&pool)
        .await
        .expect("grant visible environment");

        let hidden_environment: Uuid =
            sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
                .bind(format!("hidden-poam-{}", Uuid::new_v4()))
                .fetch_one(&pool)
                .await
                .expect("insert hidden environment");
        let hidden_system: Uuid = sqlx::query_scalar(
            "INSERT INTO systems(hostname,environment_id,public_key,derivation,is_active)
             VALUES($1,$2,$3,'',TRUE) RETURNING id",
        )
        .bind(format!("hidden-poam-{}", Uuid::new_v4()))
        .bind(hidden_environment)
        .bind(format!("ssh-hidden-{}", Uuid::new_v4()))
        .fetch_one(&pool)
        .await
        .expect("insert hidden system");
        let hidden_policy: Uuid = sqlx::query_scalar(
            "INSERT INTO deployment_policies(name,policy_type,config,enabled)
             VALUES($1,'custom_check','{}',FALSE) RETURNING id",
        )
        .bind(format!("hidden-poam-policy-{}", Uuid::new_v4()))
        .fetch_one(&pool)
        .await
        .expect("insert hidden policy");
        let hidden_finding: Uuid = sqlx::query_scalar(
            "INSERT INTO poam_findings(system_id,policy_lineage_id) VALUES($1,$2) RETURNING id",
        )
        .bind(hidden_system)
        .bind(hidden_policy)
        .fetch_one(&pool)
        .await
        .expect("insert hidden finding");
        sqlx::query(
            "INSERT INTO poam_finding_links(poam_id,finding_id,linked_by) VALUES($1,$2,$3)",
        )
        .bind(poam_id)
        .bind(hidden_finding)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("link hidden finding");

        let visible: bool = sqlx::query_scalar(
            "SELECT notification_visible_to_user_snapshot($1,'poams',$2,'environments',ARRAY[$3]::uuid[])",
        )
        .bind(user_id)
        .bind(poam_id.to_string())
        .bind(visible_environment)
        .fetch_one(&pool)
        .await
        .expect("authorize partial POA&M context");
        assert!(!visible);
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires repository-provided isolated PostgreSQL"]
    async fn poam_notification_visibility_includes_assignment_only_context(pool: PgPool) {
        let user_id = create_test_user(&pool, "assignment-only-poam-context").await;
        let poam_id = create_poam_fixture(&pool, user_id).await;
        sqlx::query("UPDATE user_role_assignments SET role='viewer' WHERE user_id=$1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        let finding_environment: Uuid = sqlx::query_scalar(
            "SELECT system.environment_id FROM poam_context_systems context
             JOIN systems system ON system.id=context.system_id WHERE context.poam_id=$1 LIMIT 1",
        )
        .bind(poam_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)",
        )
        .bind(user_id)
        .bind(finding_environment)
        .execute(&pool)
        .await
        .unwrap();
        let assignment_environment: Uuid =
            sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
                .bind(format!("a-env-{}", Uuid::new_v4()))
                .fetch_one(&pool)
                .await
                .unwrap();
        let assignment_system: Uuid = sqlx::query_scalar(
            "INSERT INTO systems(hostname,environment_id,public_key,derivation,is_active)
             VALUES($1,$2,$3,'',TRUE) RETURNING id",
        )
        .bind(format!("assignment-only-system-{}", Uuid::new_v4()))
        .bind(assignment_environment)
        .bind(format!("ssh-assignment-only-{}", Uuid::new_v4()))
        .fetch_one(&pool)
        .await
        .unwrap();
        let bundle_id: Uuid = sqlx::query_scalar(
            "INSERT INTO compliance_bundles(name,framework,layer,owner)
             VALUES($1,'test','fleet','test') RETURNING id",
        )
        .bind(format!("assignment-only-bundle-{}", Uuid::new_v4()))
        .fetch_one(&pool)
        .await
        .unwrap();
        let bundle_version_id: Uuid = sqlx::query_scalar(
            "SELECT current_draft_version_id FROM compliance_bundles WHERE id=$1",
        )
        .bind(bundle_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let assignment_id: Uuid = sqlx::query_scalar(
            "INSERT INTO compliance_bundle_assignments(bundle_version_id,bundle_id,scope_type,
                system_id,enforcement_mode,assignment_overlay_digest,created_by,updated_by)
             VALUES($1,$2,'system',$3,'enforce','test',$4,$4) RETURNING id",
        )
        .bind(bundle_version_id)
        .bind(bundle_id)
        .bind(assignment_system)
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let assignment_version_id: Uuid = sqlx::query_scalar(
            "INSERT INTO compliance_bundle_assignment_versions(assignment_id,version_number,
                bundle_version_id,enforcement_mode,assignment_overlay_digest,created_by)
             VALUES($1,1,$2,'enforce','test',$3) RETURNING id",
        )
        .bind(assignment_id)
        .bind(bundle_version_id)
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE compliance_bundle_assignments SET current_version_id=$2 WHERE id=$1")
            .bind(assignment_id)
            .bind(assignment_version_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO poam_assignment_references(poam_id,assignment_id,assignment_version_id,added_by)
             VALUES($1,$2,$3,$4)",
        )
        .bind(poam_id)
        .bind(assignment_id)
        .bind(assignment_version_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();

        let hidden: bool = sqlx::query_scalar("SELECT notification_visible_to_user($1,'poams',$2)")
            .bind(user_id)
            .bind(poam_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(!hidden);
        sqlx::query(
            "INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)",
        )
        .bind(user_id)
        .bind(assignment_environment)
        .execute(&pool)
        .await
        .unwrap();
        let visible: bool =
            sqlx::query_scalar("SELECT notification_visible_to_user($1,'poams',$2)")
                .bind(user_id)
                .bind(poam_id.to_string())
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(visible);
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires repository-provided isolated PostgreSQL"]
    async fn system_notification_authorization_follows_current_environment(pool: PgPool) {
        let user_id = create_test_user(&pool, "moved-system").await;
        let first_environment: Uuid =
            sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
                .bind(format!("moved-first-{}", Uuid::new_v4()))
                .fetch_one(&pool)
                .await
                .unwrap();
        let second_environment: Uuid =
            sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
                .bind(format!("moved-second-{}", Uuid::new_v4()))
                .fetch_one(&pool)
                .await
                .unwrap();
        let system_id: Uuid = sqlx::query_scalar(
            "INSERT INTO systems(hostname,environment_id,public_key,derivation,is_active)
             VALUES($1,$2,$3,'',TRUE) RETURNING id",
        )
        .bind(format!("moved-system-{}", Uuid::new_v4()))
        .bind(first_environment)
        .bind(format!("ssh-moved-{}", Uuid::new_v4()))
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)",
        )
        .bind(user_id)
        .bind(first_environment)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
            .bind(system_id)
            .bind(second_environment)
            .execute(&pool)
            .await
            .unwrap();

        let visible: bool = sqlx::query_scalar(
            "SELECT notification_visible_to_user_snapshot($1,'systems',$2,'environments',ARRAY[$3]::uuid[])",
        )
        .bind(user_id)
        .bind(system_id.to_string())
        .bind(first_environment)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(!visible);
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires repository-provided isolated PostgreSQL"]
    async fn concurrent_notification_bootstrap_and_producer_retry_without_deadlock(pool: PgPool) {
        let user_id = create_test_user(&pool, "bootstrap-producer-race").await;
        let poam_id = create_poam_fixture(&pool, user_id).await;
        let historical_id: Uuid = sqlx::query_scalar(
            "INSERT INTO poam_activity(poam_id,actor_user_id,kind,payload)
             VALUES($1,$2,'status_changed','{\"to\":\"awaiting_verification\"}') RETURNING id",
        )
        .bind(poam_id)
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query("DELETE FROM user_notification_source_events WHERE source_id=$1")
            .bind(historical_id)
            .execute(&pool)
            .await
            .unwrap();
        let producer_pool = pool.clone();
        let producer = tokio::spawn(async move {
            sqlx::query(
                "INSERT INTO poam_activity(poam_id,actor_user_id,kind,payload)
                 VALUES($1,$2,'status_changed','{\"to\":\"awaiting_verification\"}')",
            )
            .bind(poam_id)
            .bind(user_id)
            .execute(&producer_pool)
            .await
        });
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            materialize_attention_notifications_for_user(&pool, user_id, false),
        )
        .await
        .expect("bootstrap must not deadlock")
        .expect("bootstrap notification sources");
        tokio::time::timeout(std::time::Duration::from_secs(5), producer)
            .await
            .expect("producer must not deadlock")
            .expect("join producer")
            .expect("insert concurrent source");
        materialize_attention_notifications_for_user(&pool, user_id, false)
            .await
            .expect("retry after concurrent producer");
        let queued: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_notification_source_events
             WHERE source_kind='poam_activity' AND notification_source_id=$1",
        )
        .bind(poam_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(queued, 2);
    }
}
