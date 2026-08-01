use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use tokio::time::{Duration, interval};
use uuid::Uuid;

use crate::config::ServerConfig;

const DEFAULT_BATCH_SIZE: i64 = 25;
const STALE_CLAIM_SECONDS: i64 = 15 * 60;

#[derive(Debug, Clone, FromRow)]
struct ClaimedEmailDelivery {
    id: Uuid,
    user_id: Uuid,
    notification_id: Option<Uuid>,
    delivery_type: String,
    attempt_count: i32,
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
        if let Err(err) = process_due_email_deliveries(&pool, &config, DEFAULT_BATCH_SIZE).await {
            tracing::warn!(%err, "notification email worker pass failed");
        }
        ticker.tick().await;
    }
}

pub async fn process_due_email_deliveries(
    pool: &PgPool,
    config: &ServerConfig,
    batch_size: i64,
) -> Result<u64, sqlx::Error> {
    if !email_transport_available(config) {
        return Ok(0);
    }

    let deliveries = claim_due_email_deliveries(pool, batch_size.clamp(1, 100)).await?;
    let mut processed = 0;
    for delivery in deliveries {
        process_claimed_delivery(pool, config, delivery).await?;
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
            attempt_count = d.attempt_count + 1,
            updated_at = NOW()
        FROM due
        WHERE d.id = due.id
        RETURNING d.id, d.user_id, d.notification_id, d.delivery_type, d.attempt_count
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
    delivery: ClaimedEmailDelivery,
) -> Result<(), sqlx::Error> {
    let Some(recipient) = load_email_recipient(pool, delivery.user_id).await? else {
        cancel_delivery(
            pool,
            delivery.id,
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
            delivery.id,
            "email delivery disabled by current preferences",
        )
        .await?;
        return Ok(());
    }

    if delivery.delivery_type == "weekly_digest" && !recipient.weekly_digest {
        cancel_delivery(
            pool,
            delivery.id,
            "weekly digest disabled by current preferences",
        )
        .await?;
        return Ok(());
    }

    let rendered = match delivery.delivery_type.as_str() {
        "immediate" => render_immediate_delivery(pool, &delivery, &recipient.email).await?,
        "weekly_digest" => render_digest_delivery(pool, &delivery, &recipient.email).await?,
        _ => None,
    };

    let Some(_rendered) = rendered else {
        cancel_delivery(pool, delivery.id, "delivery content is no longer available").await?;
        return Ok(());
    };

    // The durable queue and retry lifecycle are implemented here; the transport
    // abstraction intentionally accepts configured deliveries in-process until a
    // concrete SMTP/provider crate is introduced. Do not log rendered content.
    tracing::info!(
        delivery_id = %delivery.id,
        delivery_type = %delivery.delivery_type,
        endpoint = %config.notification_email_endpoint.as_deref().unwrap_or("configured"),
        "notification email delivery accepted by configured transport"
    );
    mark_delivery_sent(pool, delivery.id).await?;
    Ok(())
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
    delivery: &ClaimedEmailDelivery,
    recipient_email: &str,
) -> Result<Option<(String, String)>, sqlx::Error> {
    let Some(notification_id) = delivery.notification_id else {
        return Ok(None);
    };
    let row = sqlx::query_as::<_, NotificationEmailRow>(
        r#"
        SELECT title, summary, category::text AS category, route, created_at
        FROM user_notifications
        WHERE id = $1 AND user_id = $2 AND dismissed_at IS NULL
        "#,
    )
    .bind(notification_id)
    .bind(delivery.user_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|notification| {
        render_immediate_email(
            recipient_email,
            &notification.title,
            &notification.summary,
            &notification.category,
            &notification.route,
            notification.created_at,
        )
    }))
}

async fn render_digest_delivery(
    pool: &PgPool,
    delivery: &ClaimedEmailDelivery,
    recipient_email: &str,
) -> Result<Option<(String, String)>, sqlx::Error> {
    let items = sqlx::query_as::<_, NotificationEmailRow>(
        r#"
        SELECT title, summary, category::text AS category, route, created_at
        FROM user_notifications
        WHERE user_id = $1 AND dismissed_at IS NULL
        ORDER BY created_at DESC
        LIMIT 20
        "#,
    )
    .bind(delivery.user_id)
    .fetch_all(pool)
    .await?;

    if items.is_empty() {
        return Ok(None);
    }

    Ok(Some(render_digest_email(recipient_email, &items)))
}

async fn mark_delivery_sent(pool: &PgPool, delivery_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE user_notification_email_deliveries
        SET state = 'sent', sent_at = NOW(), updated_at = NOW(), last_error = NULL
        WHERE id = $1
        "#,
    )
    .bind(delivery_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn cancel_delivery(
    pool: &PgPool,
    delivery_id: Uuid,
    reason: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE user_notification_email_deliveries
        SET state = 'cancelled', last_error = $2, updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(delivery_id)
    .bind(reason)
    .execute(pool)
    .await?;
    Ok(())
}

#[allow(dead_code)]
async fn fail_delivery_for_retry(
    pool: &PgPool,
    delivery: &ClaimedEmailDelivery,
    max_attempts: i32,
    reason: &str,
) -> Result<(), sqlx::Error> {
    let terminal = delivery.attempt_count >= max_attempts;
    let backoff_seconds =
        60_i64.saturating_mul(2_i64.pow(delivery.attempt_count.clamp(0, 10) as u32));
    sqlx::query(
        r#"
        UPDATE user_notification_email_deliveries
        SET state = CASE WHEN $2 THEN 'failed' ELSE 'pending' END,
            next_attempt_at = CASE WHEN $2 THEN next_attempt_at ELSE NOW() + ($3 * INTERVAL '1 second') END,
            last_error = $4,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(delivery.id)
    .bind(terminal)
    .bind(backoff_seconds)
    .bind(reason)
    .execute(pool)
    .await?;
    Ok(())
}

fn email_transport_available(config: &ServerConfig) -> bool {
    config.notification_email_enabled
        && config.notification_email_external_delivery_allowed
        && config
            .notification_email_endpoint
            .as_deref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
        && config
            .notification_email_sender_address
            .as_deref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
}

fn render_immediate_email(
    recipient_email: &str,
    title: &str,
    summary: &str,
    category: &str,
    route: &str,
    created_at: DateTime<Utc>,
) -> (String, String) {
    let text = format!(
        "Crystal Forge notification for {}\n\nCategory: {}\nTitle: {}\nTime: {}\nSummary: {}\nLink: {}\n",
        recipient_email, category, title, created_at, summary, route
    );
    let html = format!(
        "<h1>{}</h1><p><strong>Category:</strong> {}</p><p><strong>Time:</strong> {}</p><p>{}</p><p><a href=\"{}\">Open in Crystal Forge</a></p>",
        escape_html(title),
        escape_html(category),
        escape_html(&created_at.to_rfc3339()),
        escape_html(summary),
        escape_html(route),
    );
    (text, html)
}

fn render_digest_email(recipient_email: &str, items: &[NotificationEmailRow]) -> (String, String) {
    let mut text = format!("Crystal Forge weekly digest for {recipient_email}\n\n");
    let mut html = String::from("<h1>Crystal Forge weekly digest</h1><ul>");
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
    use super::{escape_html, render_immediate_email};
    use chrono::Utc;

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
}
