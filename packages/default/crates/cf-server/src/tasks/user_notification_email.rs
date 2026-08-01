use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool};
use std::{future::Future, pin::Pin};
use tokio::time::{Duration, interval};
use uuid::Uuid;

use crate::config::ServerConfig;
use cf_config::config::{
    notification_provider_endpoint_allowed, notification_public_base_url_allowed,
};

const DEFAULT_BATCH_SIZE: i64 = 25;
const STALE_CLAIM_SECONDS: i64 = 15 * 60;

#[derive(Debug, Clone, FromRow)]
struct ClaimedEmailDelivery {
    id: Uuid,
    claim_token: Uuid,
    user_id: Uuid,
    notification_id: Option<Uuid>,
    delivery_type: String,
    attempt_count: i32,
    idempotency_key: String,
}

#[derive(Debug, Clone, FromRow)]
struct EmailRecipientRow {
    email: String,
    delivery_channel: String,
    weekly_digest: bool,
}

#[derive(Debug, Clone, FromRow)]
struct NotificationEmailRow {
    title: String,
    summary: String,
    category: String,
    route: String,
    created_at: DateTime<Utc>,
}

pub async fn run_user_notification_email_loop(pool: PgPool, config: ServerConfig) {
    if !email_transport_available(&config) {
        tracing::info!("notification email worker disabled: email transport is unavailable");
        return;
    }

    let interval_seconds = config.notification_email_worker_interval_seconds.max(1);
    let mut ticker = interval(Duration::from_secs(interval_seconds));

    loop {
        if let Err(err) =
            crate::queries::user_notifications::materialize_all_user_notifications(&pool).await
        {
            tracing::warn!(%err, "notification producer pass failed");
        }
        if let Err(err) = crate::queries::user_notifications::enqueue_due_weekly_digest_deliveries(
            &pool,
            &config.notification_email_digest_schedule,
        )
        .await
        {
            tracing::warn!(%err, "weekly digest producer pass failed");
        }

        let transport = HttpEmailTransport::new(config.clone());
        if let Err(err) =
            process_due_email_deliveries(&pool, &config, &transport, DEFAULT_BATCH_SIZE).await
        {
            tracing::warn!(%err, "notification email worker pass failed");
        }
        ticker.tick().await;
    }
}

pub async fn process_due_email_deliveries(
    pool: &PgPool,
    config: &ServerConfig,
    transport: &(dyn EmailTransport + Send + Sync),
    batch_size: i64,
) -> Result<u64, sqlx::Error> {
    if !email_transport_available(config) {
        return Ok(0);
    }

    let deliveries = claim_due_email_deliveries(pool, batch_size.clamp(1, 100)).await?;
    let mut processed = 0;
    for delivery in deliveries {
        process_claimed_delivery(pool, config, transport, delivery).await?;
        processed += 1;
    }
    Ok(processed)
}

async fn claim_due_email_deliveries(
    pool: &PgPool,
    batch_size: i64,
) -> Result<Vec<ClaimedEmailDelivery>, sqlx::Error> {
    sqlx::query_as::<_, ClaimedEmailDelivery>(
        r#"
        WITH due AS (
            SELECT id
            FROM user_notification_email_deliveries
            WHERE (
                    state = 'pending'
                    AND next_attempt_at <= NOW()
                  )
               OR (
                    state = 'sending'
                    AND claimed_at < NOW() - ($2 * INTERVAL '1 second')
                  )
            ORDER BY next_attempt_at ASC, created_at ASC
            LIMIT $1
            FOR UPDATE SKIP LOCKED
        )
        UPDATE user_notification_email_deliveries d
        SET state = 'sending',
            claimed_at = NOW(),
            claim_token = gen_random_uuid(),
            attempt_count = d.attempt_count + 1,
            updated_at = NOW()
        FROM due
        WHERE d.id = due.id
        RETURNING d.id, d.claim_token, d.user_id, d.notification_id, d.delivery_type, d.attempt_count,
                  d.idempotency_key
        "#,
    )
    .bind(batch_size)
    .bind(STALE_CLAIM_SECONDS)
    .fetch_all(pool)
    .await
}

async fn process_claimed_delivery(
    pool: &PgPool,
    config: &ServerConfig,
    transport: &(dyn EmailTransport + Send + Sync),
    delivery: ClaimedEmailDelivery,
) -> Result<(), sqlx::Error> {
    let Some(recipient) = load_email_recipient(pool, delivery.user_id).await? else {
        cancel_delivery(
            pool,
            &delivery,
            "recipient email or preferences unavailable",
        )
        .await?;
        return Ok(());
    };

    if !recipient.delivery_channel.eq_ignore_ascii_case("email")
        && !recipient.delivery_channel.eq_ignore_ascii_case("both")
    {
        cancel_delivery(
            pool,
            &delivery,
            "email delivery disabled by current preferences",
        )
        .await?;
        return Ok(());
    }

    if delivery.delivery_type == "weekly_digest" && !recipient.weekly_digest {
        cancel_delivery(
            pool,
            &delivery,
            "weekly digest disabled by current preferences",
        )
        .await?;
        return Ok(());
    }

    let rendered = match delivery.delivery_type.as_str() {
        "immediate" => render_immediate_delivery(pool, config, &delivery, &recipient.email).await?,
        "weekly_digest" => {
            render_digest_delivery(pool, config, &delivery, &recipient.email).await?
        }
        _ => None,
    };

    let Some((subject, text_body, html_body)) = rendered else {
        cancel_delivery(pool, &delivery, "delivery content is no longer available").await?;
        return Ok(());
    };

    let message = EmailMessage {
        idempotency_key: delivery.idempotency_key.clone(),
        to: recipient.email,
        from: config
            .notification_email_sender_address
            .clone()
            .unwrap_or_default(),
        from_name: config.notification_email_sender_name.clone(),
        subject,
        text_body,
        html_body,
    };

    match transport.send(message).await {
        Ok(receipt) => {
            tracing::info!(delivery_id = %delivery.id, %receipt, "notification email accepted by transport");
            mark_delivery_sent(pool, &delivery).await?;
        }
        Err(err) => {
            fail_delivery_for_retry(
                pool,
                &delivery,
                config.notification_email_max_attempts,
                &err,
            )
            .await?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct EmailMessage {
    pub idempotency_key: String,
    pub to: String,
    pub from: String,
    pub from_name: String,
    pub subject: String,
    pub text_body: String,
    pub html_body: String,
}

pub trait EmailTransport {
    fn send<'a>(
        &'a self,
        message: EmailMessage,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>>;
}

#[derive(Debug, Clone)]
struct HttpEmailTransport {
    config: ServerConfig,
}

impl HttpEmailTransport {
    fn new(config: ServerConfig) -> Self {
        Self { config }
    }
}

impl EmailTransport for HttpEmailTransport {
    fn send<'a>(
        &'a self,
        message: EmailMessage,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        Box::pin(async move {
            let endpoint = self
                .config
                .notification_email_endpoint
                .as_deref()
                .ok_or_else(|| "email endpoint is not configured".to_string())?;
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(
                    self.config
                        .notification_email_request_timeout_seconds
                        .max(1),
                ))
                .build()
                .map_err(|err| format!("email transport client build failed: {err}"))?;
            let response = client
                .post(endpoint)
                .header("Idempotency-Key", &message.idempotency_key)
                .bearer_auth(load_provider_token(&self.config).await?)
                .json(&message)
                .send()
                .await
                .map_err(|err| format!("email transport request failed: {err}"))?;
            let status = response.status();
            if status.is_success() {
                Ok(format!("http:{status}"))
            } else {
                Err(format!("email transport rejected message with {status}"))
            }
        })
    }
}

async fn load_email_recipient(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Option<EmailRecipientRow>, sqlx::Error> {
    sqlx::query_as::<_, EmailRecipientRow>(
        r#"
        SELECT u.email, p.delivery_channel::text AS delivery_channel, p.weekly_digest
        FROM users u
        JOIN user_notification_preferences p ON p.user_id = u.id
        WHERE u.id = $1 AND btrim(u.email) <> ''
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

async fn render_immediate_delivery(
    pool: &PgPool,
    config: &ServerConfig,
    delivery: &ClaimedEmailDelivery,
    recipient_email: &str,
) -> Result<Option<(String, String, String)>, sqlx::Error> {
    let Some(notification_id) = delivery.notification_id else {
        return Ok(None);
    };
    let row = sqlx::query_as::<_, NotificationEmailRow>(
        r#"
        SELECT user_notifications.title, user_notifications.summary,
               user_notifications.category::text AS category,
               user_notifications.route, user_notifications.created_at
        FROM user_notifications
        JOIN user_notification_preferences p
          ON p.user_id = user_notifications.user_id
        WHERE user_notifications.id = $1
          AND user_notifications.user_id = $2
          AND user_notifications.dismissed_at IS NULL
          AND notification_visible_to_user($2, user_notifications.source_type, user_notifications.source_id)
          AND (
                (user_notifications.category = 'deploy_failures' AND p.deploy_failures)
             OR (user_notifications.category = 'build_failures' AND p.build_failures)
             OR (user_notifications.category = 'critical_cves' AND p.critical_cves)
             OR (user_notifications.category = 'policy_violations' AND p.policy_violations)
             OR (user_notifications.category = 'heartbeat_lost' AND p.heartbeat_lost)
          )
          AND user_notifications.created_at >= CASE user_notifications.category
                WHEN 'deploy_failures' THEN p.deploy_failures_email_enabled_at
                WHEN 'build_failures' THEN p.build_failures_email_enabled_at
                WHEN 'critical_cves' THEN p.critical_cves_email_enabled_at
                WHEN 'policy_violations' THEN p.policy_violations_email_enabled_at
                WHEN 'heartbeat_lost' THEN p.heartbeat_lost_email_enabled_at
          END
        "#,
    )
    .bind(notification_id)
    .bind(delivery.user_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|notification| {
        let (text, html) = render_immediate_email(
            recipient_email,
            &notification.title,
            &notification.summary,
            &notification.category,
            &absolute_email_link(config, &notification.route),
            notification.created_at,
        );
        (notification.title, text, html)
    }))
}

async fn render_digest_delivery(
    pool: &PgPool,
    config: &ServerConfig,
    delivery: &ClaimedEmailDelivery,
    recipient_email: &str,
) -> Result<Option<(String, String, String)>, sqlx::Error> {
    let Some((period_start, period_end)) = sqlx::query_as::<_, (DateTime<Utc>, DateTime<Utc>)>(
        "SELECT GREATEST(r.period_start, p.weekly_digest_enabled_at), r.period_end
         FROM user_notification_weekly_digest_runs r
         JOIN user_notification_preferences p ON p.user_id = r.user_id
         WHERE r.delivery_id = $1
           AND r.user_id = $2
           AND p.weekly_digest = TRUE
           AND p.delivery_channel IN ('email', 'both')
           AND p.weekly_digest_enabled_at IS NOT NULL
           AND p.weekly_digest_enabled_at < r.period_end
         LIMIT 1",
    )
    .bind(delivery.id)
    .bind(delivery.user_id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };

    let counts = sqlx::query_as::<_, (String, i64)>(
        r#"
        SELECT user_notifications.category::text AS category, COUNT(*) AS count
        FROM user_notifications
        JOIN user_notification_preferences p
          ON p.user_id = user_notifications.user_id
        WHERE user_notifications.user_id = $1
          AND user_notifications.dismissed_at IS NULL
          AND user_notifications.created_at >= $2
          AND user_notifications.created_at < $3
          AND notification_visible_to_user($1, user_notifications.source_type, user_notifications.source_id)
          AND (
                (user_notifications.category = 'deploy_failures' AND p.deploy_failures)
             OR (user_notifications.category = 'build_failures' AND p.build_failures)
             OR (user_notifications.category = 'critical_cves' AND p.critical_cves)
             OR (user_notifications.category = 'policy_violations' AND p.policy_violations)
             OR (user_notifications.category = 'heartbeat_lost' AND p.heartbeat_lost)
          )
          AND user_notifications.created_at >= CASE user_notifications.category
                WHEN 'deploy_failures' THEN p.deploy_failures_email_enabled_at
                WHEN 'build_failures' THEN p.build_failures_email_enabled_at
                WHEN 'critical_cves' THEN p.critical_cves_email_enabled_at
                WHEN 'policy_violations' THEN p.policy_violations_email_enabled_at
                WHEN 'heartbeat_lost' THEN p.heartbeat_lost_email_enabled_at
          END
        GROUP BY user_notifications.category
        ORDER BY user_notifications.category
        "#,
    )
    .bind(delivery.user_id)
    .bind(period_start)
    .bind(period_end)
    .fetch_all(pool)
    .await?;

    let mut items = sqlx::query_as::<_, NotificationEmailRow>(
        r#"
        SELECT user_notifications.title, user_notifications.summary,
               user_notifications.category::text AS category,
               user_notifications.route, user_notifications.created_at
        FROM user_notifications
        JOIN user_notification_preferences p
          ON p.user_id = user_notifications.user_id
        WHERE user_notifications.user_id = $1
          AND user_notifications.dismissed_at IS NULL
          AND user_notifications.created_at >= $2
          AND user_notifications.created_at < $3
          AND notification_visible_to_user($1, user_notifications.source_type, user_notifications.source_id)
          AND (
                (user_notifications.category = 'deploy_failures' AND p.deploy_failures)
             OR (user_notifications.category = 'build_failures' AND p.build_failures)
             OR (user_notifications.category = 'critical_cves' AND p.critical_cves)
             OR (user_notifications.category = 'policy_violations' AND p.policy_violations)
             OR (user_notifications.category = 'heartbeat_lost' AND p.heartbeat_lost)
          )
          AND user_notifications.created_at >= CASE user_notifications.category
                WHEN 'deploy_failures' THEN p.deploy_failures_email_enabled_at
                WHEN 'build_failures' THEN p.build_failures_email_enabled_at
                WHEN 'critical_cves' THEN p.critical_cves_email_enabled_at
                WHEN 'policy_violations' THEN p.policy_violations_email_enabled_at
                WHEN 'heartbeat_lost' THEN p.heartbeat_lost_email_enabled_at
          END
        ORDER BY user_notifications.created_at DESC
        LIMIT 20
        "#,
    )
    .bind(delivery.user_id)
    .bind(period_start)
    .bind(period_end)
    .fetch_all(pool)
    .await?;

    if counts.is_empty() || items.is_empty() {
        return Ok(None);
    }
    for item in &mut items {
        item.route = absolute_email_link(config, &item.route);
    }

    let (text, html) =
        render_digest_email(recipient_email, period_start, period_end, &counts, &items);
    Ok(Some((
        "Crystal Forge weekly digest".to_string(),
        text,
        html,
    )))
}

async fn mark_delivery_sent(
    pool: &PgPool,
    delivery: &ClaimedEmailDelivery,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let updated = sqlx::query(
        r#"
        UPDATE user_notification_email_deliveries
        SET state = 'sent', sent_at = NOW(), updated_at = NOW(), last_error = NULL, claim_token = NULL
        WHERE id = $1 AND claim_token = $2 AND state = 'sending'
        "#,
    )
    .bind(delivery.id)
    .bind(delivery.claim_token)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        tx.rollback().await?;
        tracing::warn!(delivery_id = %delivery.id, "notification email completion ignored after stale claim");
        return Ok(());
    }
    sqlx::query(
        r#"
        UPDATE user_notification_weekly_digest_runs
        SET status = 'sent', sent_at = NOW(), error_details = NULL
        WHERE delivery_id = $1
        "#,
    )
    .bind(delivery.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn cancel_delivery(
    pool: &PgPool,
    delivery: &ClaimedEmailDelivery,
    reason: &str,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let updated = sqlx::query(
        r#"
        UPDATE user_notification_email_deliveries
        SET state = 'cancelled', last_error = $3, updated_at = NOW(), claim_token = NULL
        WHERE id = $1 AND claim_token = $2 AND state = 'sending'
        "#,
    )
    .bind(delivery.id)
    .bind(delivery.claim_token)
    .bind(reason)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        tx.rollback().await?;
        tracing::warn!(delivery_id = %delivery.id, "notification email cancellation ignored after stale claim");
        return Ok(());
    }
    sqlx::query(
        r#"
        UPDATE user_notification_weekly_digest_runs
        SET status = 'skipped', error_details = $2
        WHERE delivery_id = $1
        "#,
    )
    .bind(delivery.id)
    .bind(reason)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn fail_delivery_for_retry(
    pool: &PgPool,
    delivery: &ClaimedEmailDelivery,
    max_attempts: i32,
    reason: &str,
) -> Result<(), sqlx::Error> {
    let terminal = delivery.attempt_count >= max_attempts;
    let backoff_seconds =
        60_i64.saturating_mul(2_i64.pow(delivery.attempt_count.clamp(0, 10) as u32));
    let mut tx = pool.begin().await?;
    let updated = sqlx::query(
        r#"
        UPDATE user_notification_email_deliveries
        SET state = CASE WHEN $2 THEN 'failed' ELSE 'pending' END,
            next_attempt_at = CASE WHEN $2 THEN next_attempt_at ELSE NOW() + ($3 * INTERVAL '1 second') END,
            last_error = $4,
            updated_at = NOW(),
            claim_token = NULL
        WHERE id = $1 AND claim_token = $5 AND state = 'sending'
        "#,
    )
    .bind(delivery.id)
    .bind(terminal)
    .bind(backoff_seconds)
    .bind(reason)
    .bind(delivery.claim_token)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        tx.rollback().await?;
        tracing::warn!(delivery_id = %delivery.id, "notification email failure ignored after stale claim");
        return Ok(());
    }
    sqlx::query(
        r#"
        UPDATE user_notification_weekly_digest_runs
        SET status = CASE WHEN $2 THEN 'failed' ELSE 'pending' END,
            error_details = $3
        WHERE delivery_id = $1
        "#,
    )
    .bind(delivery.id)
    .bind(terminal)
    .bind(reason)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

fn email_transport_available(config: &ServerConfig) -> bool {
    config.notification_email_enabled
        && config.notification_email_external_delivery_allowed
        && config
            .notification_email_endpoint
            .as_deref()
            .map(|value| {
                notification_provider_endpoint_allowed(
                    value,
                    config.notification_email_allow_insecure_loopback,
                )
            })
            .unwrap_or(false)
        && config
            .public_base_url
            .as_deref()
            .map(notification_public_base_url_allowed)
            .unwrap_or(false)
        && config
            .notification_email_provider_token_file
            .as_ref()
            .map(|path| !path.as_os_str().is_empty() && !path.starts_with("/nix/store"))
            .unwrap_or(false)
        && config
            .notification_email_sender_address
            .as_deref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
}

async fn load_provider_token(config: &ServerConfig) -> Result<String, String> {
    let path = config
        .notification_email_provider_token_file
        .as_ref()
        .ok_or_else(|| "email provider token file is not configured".to_string())?;
    let token = tokio::fs::read_to_string(path)
        .await
        .map_err(|err| format!("email provider token file could not be read: {err}"))?;
    let token = token.trim();
    if token.is_empty() {
        Err("email provider token file is empty".to_string())
    } else {
        Ok(token.to_string())
    }
}

fn absolute_email_link(config: &ServerConfig, route: &str) -> String {
    let base = config
        .public_base_url
        .as_deref()
        .unwrap_or("")
        .trim_end_matches('/');
    let route = if route.starts_with('/') {
        route.to_string()
    } else {
        format!("/{route}")
    };
    format!("{base}{route}")
}

fn render_immediate_email(
    recipient_email: &str,
    title: &str,
    summary: &str,
    category: &str,
    route: &str,
    created_at: DateTime<Utc>,
) -> (String, String) {
    let severity = notification_severity(category);
    let text = format!(
        "Crystal Forge notification for {}\n\nCategory: {}\nSeverity: {}\nTitle: {}\nTime: {}\nSummary: {}\nLink: {}\n",
        recipient_email, category, severity, title, created_at, summary, route
    );
    let html = format!(
        "<h1>{}</h1><p><strong>Category:</strong> {}</p><p><strong>Severity:</strong> {}</p><p><strong>Time:</strong> {}</p><p>{}</p><p><a href=\"{}\">Open in Crystal Forge</a></p>",
        escape_html(title),
        escape_html(category),
        escape_html(severity),
        escape_html(&created_at.to_rfc3339()),
        escape_html(summary),
        escape_html(route),
    );
    (text, html)
}

fn render_digest_email(
    recipient_email: &str,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
    counts: &[(String, i64)],
    items: &[NotificationEmailRow],
) -> (String, String) {
    let mut text = format!(
        "Crystal Forge weekly digest for {recipient_email}\n\nPeriod: {} to {}\nCounts by category:\n{}\n",
        period_start.to_rfc3339(),
        period_end.to_rfc3339(),
        counts
            .iter()
            .map(|(category, count)| format!("- {category}: {count}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let mut html = format!(
        "<h1>Crystal Forge weekly digest</h1><p><strong>Period:</strong> {} to {}</p><h2>Counts by category</h2><ul>{}</ul><h2>Recent items</h2><ul>",
        escape_html(&period_start.to_rfc3339()),
        escape_html(&period_end.to_rfc3339()),
        counts
            .iter()
            .map(|(category, count)| format!("<li>{}: {}</li>", escape_html(category), count))
            .collect::<Vec<_>>()
            .join("")
    );
    for item in items.iter().take(20) {
        text.push_str(&format!(
            "- [{}] {} — {} ({})\n",
            item.category, item.title, item.summary, item.route
        ));
        html.push_str(&format!(
            "<li><strong>{}</strong>: {} <a href=\"{}\">Open</a></li>",
            escape_html(&item.category),
            escape_html(&item.title),
            escape_html(&item.route),
        ));
    }
    html.push_str("</ul>");
    (text, html)
}

fn notification_severity(category: &str) -> &'static str {
    match category {
        "critical_cves" => "critical",
        "deploy_failures" | "build_failures" => "high",
        "policy_violations" | "heartbeat_lost" => "warning",
        _ => "info",
    }
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::{
        EmailMessage, EmailTransport, escape_html, process_due_email_deliveries,
        render_immediate_email,
    };
    use chrono::Utc;
    use sqlx::{PgPool, postgres::PgPoolOptions};
    use std::{
        future::Future,
        pin::Pin,
        sync::{Arc, Mutex},
    };
    use uuid::Uuid;

    #[derive(Clone)]
    struct FakeEmailTransport {
        result: Result<String, String>,
        messages: Arc<Mutex<Vec<EmailMessage>>>,
    }

    impl FakeEmailTransport {
        fn accepting() -> Self {
            Self {
                result: Ok("fake-accepted".to_string()),
                messages: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn rejecting(error: &str) -> Self {
            Self {
                result: Err(error.to_string()),
                messages: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn sent_count(&self) -> usize {
            self.messages.lock().expect("fake messages lock").len()
        }

        fn text_bodies(&self) -> Vec<String> {
            self.messages
                .lock()
                .expect("fake messages lock")
                .iter()
                .map(|message| message.text_body.clone())
                .collect()
        }
    }

    impl EmailTransport for FakeEmailTransport {
        fn send<'a>(
            &'a self,
            message: EmailMessage,
        ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
            Box::pin(async move {
                self.messages
                    .lock()
                    .expect("fake messages lock")
                    .push(message);
                self.result.clone()
            })
        }
    }

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

    fn email_config(max_attempts: i32) -> crate::config::ServerConfig {
        crate::config::ServerConfig {
            notification_email_enabled: true,
            notification_email_external_delivery_allowed: true,
            notification_email_endpoint: Some("http://127.0.0.1:9/fake".to_string()),
            notification_email_allow_insecure_loopback: true,
            public_base_url: Some("https://crystal-forge.example.test".to_string()),
            notification_email_provider_token_file: Some("/run/secrets/fake-email-token".into()),
            notification_email_sender_address: Some("noreply@example.test".to_string()),
            notification_email_max_attempts: max_attempts,
            ..Default::default()
        }
    }

    async fn insert_queued_immediate_delivery(pool: &PgPool, attempt_count: i32) -> (Uuid, Uuid) {
        let user_id = Uuid::new_v4();
        let notification_id = Uuid::new_v4();
        let source_id = Uuid::new_v4().to_string();

        sqlx::query(
            "INSERT INTO users (id, username, first_name, last_name, email)
             VALUES ($1, $2, 'Email', 'Tester', $3)",
        )
        .bind(user_id)
        .bind(format!("email-{user_id}"))
        .bind(format!("{user_id}@example.test"))
        .execute(pool)
        .await
        .expect("insert test user");

        sqlx::query(
            "INSERT INTO user_notification_preferences (user_id, delivery_channel)
             VALUES ($1, 'email')",
        )
        .bind(user_id)
        .execute(pool)
        .await
        .expect("insert notification preferences");

        sqlx::query(
            "INSERT INTO user_notifications
                (id, user_id, category, source_type, source_id, title, summary, route, in_app_visible)
             VALUES ($1, $2, 'build_failures', 'builds', $3, 'Build failed', 'The build failed.', '/builds', FALSE)",
        )
        .bind(notification_id)
        .bind(user_id)
        .bind(source_id)
        .execute(pool)
        .await
        .expect("insert notification");

        let (delivery_id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO user_notification_email_deliveries
                (user_id, notification_id, delivery_type, idempotency_key, attempt_count, next_attempt_at)
             VALUES ($1, $2, 'immediate', $3, $4, NOW() - INTERVAL '1 second')
             RETURNING id",
        )
        .bind(user_id)
        .bind(notification_id)
        .bind(format!("test-immediate:{notification_id}"))
        .bind(attempt_count)
        .fetch_one(pool)
        .await
        .expect("insert email delivery");

        (delivery_id, user_id)
    }

    async fn insert_queued_weekly_digest_delivery(pool: &PgPool) -> Uuid {
        let user_id = Uuid::new_v4();
        let in_period_id = Uuid::new_v4();
        let outside_period_id = Uuid::new_v4();
        let period_start = Utc::now() - chrono::Duration::days(7);
        let period_end = Utc::now();

        sqlx::query(
            "INSERT INTO users (id, username, first_name, last_name, email)
             VALUES ($1, $2, 'Digest', 'Tester', $3)",
        )
        .bind(user_id)
        .bind(format!("digest-{user_id}"))
        .bind(format!("digest-{user_id}@example.test"))
        .execute(pool)
        .await
        .expect("insert test user");

        sqlx::query(
            "INSERT INTO user_notification_preferences (user_id, weekly_digest, delivery_channel)
             VALUES ($1, TRUE, 'email')",
        )
        .bind(user_id)
        .execute(pool)
        .await
        .expect("insert notification preferences");

        sqlx::query(
            "INSERT INTO user_notifications
                (id, user_id, category, source_type, source_id, title, summary, route, created_at)
             VALUES
                ($1, $2, 'build_failures', 'builds', $3, 'Included digest item', 'Inside period.', '/builds', $4),
                ($5, $2, 'build_failures', 'builds', $6, 'Excluded digest item', 'Outside period.', '/builds', $7)",
        )
        .bind(in_period_id)
        .bind(user_id)
        .bind(Uuid::new_v4().to_string())
        .bind(period_start + chrono::Duration::hours(1))
        .bind(outside_period_id)
        .bind(Uuid::new_v4().to_string())
        .bind(period_start - chrono::Duration::hours(1))
        .execute(pool)
        .await
        .expect("insert digest notifications");

        let (delivery_id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO user_notification_email_deliveries
                (user_id, delivery_type, idempotency_key, next_attempt_at)
             VALUES ($1, 'weekly_digest', $2, NOW() - INTERVAL '1 second')
             RETURNING id",
        )
        .bind(user_id)
        .bind(format!("test-weekly-digest:{user_id}"))
        .fetch_one(pool)
        .await
        .expect("insert weekly digest delivery");

        sqlx::query(
            "INSERT INTO user_notification_weekly_digest_runs
                (user_id, period_start, period_end, status, delivery_id)
             VALUES ($1, $2, $3, 'pending', $4)",
        )
        .bind(user_id)
        .bind(period_start)
        .bind(period_end)
        .bind(delivery_id)
        .execute(pool)
        .await
        .expect("insert weekly digest run");

        delivery_id
    }

    #[test]
    fn user_notifications_email_html_escapes_controlled_values() {
        let (_text, html) = render_immediate_email(
            "alice@example.com",
            "<build failed>",
            "Package output contains <secret> & quotes",
            "build_failures",
            "/builds?name=<bad>",
            Utc::now(),
        );

        assert!(html.contains("&lt;build failed&gt;"));
        assert!(html.contains("&lt;secret&gt; &amp; quotes"));
        assert!(html.contains("/builds?name=&lt;bad&gt;"));
        assert!(!html.contains("<secret>"));
    }

    #[test]
    fn user_notifications_escape_html_handles_quotes() {
        assert_eq!(
            escape_html("'quoted' & \"double\""),
            "&#39;quoted&#39; &amp; &quot;double&quot;"
        );
    }

    #[tokio::test]
    #[ignore = "requires migrated CRYSTAL_FORGE_TEST_DATABASE_URL"]
    async fn user_notifications_email_worker_marks_sent_only_after_transport_acceptance() {
        let pool = test_pool().await;
        let (delivery_id, _user_id) = insert_queued_immediate_delivery(&pool, 0).await;
        let transport = FakeEmailTransport::accepting();

        let processed = process_due_email_deliveries(&pool, &email_config(3), &transport, 10)
            .await
            .expect("process due deliveries");

        assert_eq!(processed, 1);
        assert_eq!(transport.sent_count(), 1);
        let (state, attempt_count, sent): (String, i32, bool) = sqlx::query_as(
            "SELECT state, attempt_count, sent_at IS NOT NULL
             FROM user_notification_email_deliveries
             WHERE id = $1",
        )
        .bind(delivery_id)
        .fetch_one(&pool)
        .await
        .expect("load delivery state");
        assert_eq!(state, "sent");
        assert_eq!(attempt_count, 1);
        assert!(sent);
    }

    #[tokio::test]
    #[ignore = "requires migrated CRYSTAL_FORGE_TEST_DATABASE_URL"]
    async fn user_notifications_email_worker_retries_transport_rejection() {
        let pool = test_pool().await;
        let (delivery_id, _user_id) = insert_queued_immediate_delivery(&pool, 0).await;
        let transport = FakeEmailTransport::rejecting("provider unavailable");

        let processed = process_due_email_deliveries(&pool, &email_config(3), &transport, 10)
            .await
            .expect("process due deliveries");

        assert_eq!(processed, 1);
        assert_eq!(transport.sent_count(), 1);
        let (state, attempt_count, sent, last_error): (String, i32, bool, Option<String>) =
            sqlx::query_as(
                "SELECT state, attempt_count, sent_at IS NOT NULL, last_error
             FROM user_notification_email_deliveries
             WHERE id = $1",
            )
            .bind(delivery_id)
            .fetch_one(&pool)
            .await
            .expect("load delivery state");
        assert_eq!(state, "pending");
        assert_eq!(attempt_count, 1);
        assert!(!sent);
        assert_eq!(last_error.as_deref(), Some("provider unavailable"));
    }

    #[tokio::test]
    #[ignore = "requires migrated CRYSTAL_FORGE_TEST_DATABASE_URL"]
    async fn user_notifications_email_worker_terminally_fails_at_max_attempts() {
        let pool = test_pool().await;
        let (delivery_id, _user_id) = insert_queued_immediate_delivery(&pool, 2).await;
        let transport = FakeEmailTransport::rejecting("provider rejected");

        let processed = process_due_email_deliveries(&pool, &email_config(3), &transport, 10)
            .await
            .expect("process due deliveries");

        assert_eq!(processed, 1);
        let (state, attempt_count, sent, last_error): (String, i32, bool, Option<String>) =
            sqlx::query_as(
                "SELECT state, attempt_count, sent_at IS NOT NULL, last_error
             FROM user_notification_email_deliveries
             WHERE id = $1",
            )
            .bind(delivery_id)
            .fetch_one(&pool)
            .await
            .expect("load delivery state");
        assert_eq!(state, "failed");
        assert_eq!(attempt_count, 3);
        assert!(!sent);
        assert_eq!(last_error.as_deref(), Some("provider rejected"));
    }

    #[tokio::test]
    #[ignore = "requires migrated CRYSTAL_FORGE_TEST_DATABASE_URL"]
    async fn user_notifications_weekly_digest_respects_recorded_period() {
        let pool = test_pool().await;
        let delivery_id = insert_queued_weekly_digest_delivery(&pool).await;
        let transport = FakeEmailTransport::accepting();

        let processed = process_due_email_deliveries(&pool, &email_config(3), &transport, 10)
            .await
            .expect("process due deliveries");

        assert_eq!(processed, 1);
        let bodies = transport.text_bodies();
        assert_eq!(bodies.len(), 1);
        assert!(bodies[0].contains("Included digest item"));
        assert!(!bodies[0].contains("Excluded digest item"));
        let (delivery_state, run_status): (String, String) = sqlx::query_as(
            "SELECT d.state, r.status
             FROM user_notification_email_deliveries d
             JOIN user_notification_weekly_digest_runs r ON r.delivery_id = d.id
             WHERE d.id = $1",
        )
        .bind(delivery_id)
        .fetch_one(&pool)
        .await
        .expect("load digest state");
        assert_eq!(delivery_state, "sent");
        assert_eq!(run_status, "sent");
    }
}
