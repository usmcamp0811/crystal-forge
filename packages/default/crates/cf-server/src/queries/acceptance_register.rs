//! Reads source-owned acceptance decisions for a shared register.
//!
//! This projection does not confer decision authority. Mutations must use the
//! original policy waiver or CVE disposition service and recheck current scope.

use anyhow::anyhow;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

/// Selects one source family, or all source families when absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AcceptanceSource {
    /// Policy finding waiver, including non-accepted lifecycle states.
    PolicyWaiver,
    /// Accepted host-level CVE decision, including retired history.
    CveHost,
    /// Accepted environment-level CVE decision, including retired history.
    CveEnvironment,
}

impl AcceptanceSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::PolicyWaiver => "policy_waiver",
            Self::CveHost => "cve_host",
            Self::CveEnvironment => "cve_environment",
        }
    }
}

/// Defines bounded filters for an acceptance register read.
#[derive(Debug, Default, Deserialize)]
pub struct AcceptanceListQuery {
    /// Restricts results to one source family.
    pub source: Option<AcceptanceSource>,
    /// Restricts results to one source-native status (`accepted` for CVE).
    pub status: Option<String>,
    /// Restricts results to the original environment scope, or the host's
    /// current environment for host decisions. Waivers have no environment scope.
    pub environment_id: Option<Uuid>,
    /// Requests 1 through 100 rows; defaults to 25.
    pub limit: Option<i64>,
    /// Skips this many rows in the filtered, stable order; defaults to zero.
    pub offset: Option<i64>,
}

/// Reports one source-owned decision without inventing cross-source authority.
#[derive(Debug, Serialize)]
pub struct AcceptanceEntry {
    /// Names the owning table and authorization policy.
    pub source: AcceptanceSource,
    /// Gives the unchanged source row UUID, including for retired decisions.
    pub source_id: Uuid,
    /// Gives the waiver's last status-change timestamp for optimistic checks.
    /// CVE decisions use the immutable `source_id` as their decision version.
    pub waiver_updated_at: Option<DateTime<Utc>>,
    /// Gives the waiver lifecycle state or the CVE decision state `accepted`.
    pub status: String,
    /// Gives the source's stable policy finding ID, when applicable.
    pub finding_id: Option<Uuid>,
    /// Gives the original system scope for a waiver or host decision.
    pub system_id: Option<Uuid>,
    /// Gives the original environment scope only for environment decisions.
    /// Host and waiver decisions do not store an original environment ID.
    pub environment_id: Option<Uuid>,
    /// Gives the source policy lineage for a waiver.
    pub policy_lineage_id: Option<Uuid>,
    /// Gives the exact policy version on a waiver.
    pub policy_version_id: Option<Uuid>,
    /// Gives the CVE's canonical ID on a CVE decision.
    pub canonical_cve_id: Option<String>,
    /// Gives the CVE's canonical package name on a CVE decision.
    pub canonical_package_name: Option<String>,
    /// Gives the persisted decision justification.
    pub justification: String,
    /// Gives the actual CVE review date, if one was recorded. Waivers have none.
    pub review_date: Option<NaiveDate>,
    /// Gives the independent policy-waiver review deadline after renewal.
    pub review_due_at: Option<NaiveDate>,
    /// Gives the actual waiver authorization expiry, if set. CVEs have none.
    pub expires_at: Option<DateTime<Utc>>,
    /// Identifies the user who accepted the decision, if accepted.
    pub accepted_by: Option<Uuid>,
    /// Gives the actual acceptance time, if accepted.
    pub accepted_at: Option<DateTime<Utc>>,
    /// Gives the actual source retirement time for a historical CVE decision.
    pub retired_at: Option<DateTime<Utc>>,
    /// Identifies who retired a historical CVE decision.
    pub retired_by: Option<Uuid>,
    /// Gives the source's recorded reason for CVE retirement.
    pub retirement_reason: Option<String>,
    /// Identifies a plan only when a committed replacement link or event exists.
    pub replacement_poam_id: Option<Uuid>,
    /// Gives the waiver creation time, or the CVE decision acceptance time.
    pub recorded_at: DateTime<Utc>,
}

/// Contains one page and the complete filtered total in the same snapshot.
#[derive(Debug, Serialize)]
pub struct AcceptancePage {
    /// Contains at most `limit` entries.
    pub items: Vec<AcceptanceEntry>,
    /// Counts every authorized row matching the filters before pagination.
    pub total: i64,
    /// Gives the effective page size.
    pub limit: i64,
    /// Gives the effective zero-based offset.
    pub offset: i64,
    /// Reports whether another row follows the current page.
    pub has_more: bool,
}

/// Distinguishes a rejected register read from a database or projection fault.
#[derive(Debug)]
pub enum AcceptanceReadError {
    /// Rejects invalid source filters or pagination.
    Validation(&'static str),
    /// Rejects an inactive actor or a user without a reader role.
    Forbidden,
    /// Reports a database failure without exposing its details to the client.
    Database(sqlx::Error),
    /// Reports a source projection that violates its own typed contract.
    Projection(anyhow::Error),
}

impl std::fmt::Display for AcceptanceReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validation(message) => write!(f, "invalid acceptance query: {message}"),
            Self::Forbidden => f.write_str("actor lacks an active read role"),
            Self::Database(error) => std::fmt::Display::fmt(error, f),
            Self::Projection(error) => std::fmt::Display::fmt(error, f),
        }
    }
}

impl std::error::Error for AcceptanceReadError {}

impl From<sqlx::Error> for AcceptanceReadError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl From<anyhow::Error> for AcceptanceReadError {
    fn from(error: anyhow::Error) -> Self {
        Self::Projection(error)
    }
}

#[derive(FromRow)]
struct PageRow {
    total: i64,
    source: Option<String>,
    source_id: Option<Uuid>,
    waiver_updated_at: Option<DateTime<Utc>>,
    status: Option<String>,
    finding_id: Option<Uuid>,
    system_id: Option<Uuid>,
    environment_id: Option<Uuid>,
    policy_lineage_id: Option<Uuid>,
    policy_version_id: Option<Uuid>,
    canonical_cve_id: Option<String>,
    canonical_package_name: Option<String>,
    justification: Option<String>,
    review_date: Option<NaiveDate>,
    review_due_at: Option<NaiveDate>,
    expires_at: Option<DateTime<Utc>>,
    accepted_by: Option<Uuid>,
    accepted_at: Option<DateTime<Utc>>,
    retired_at: Option<DateTime<Utc>>,
    retired_by: Option<Uuid>,
    retirement_reason: Option<String>,
    replacement_poam_id: Option<Uuid>,
    recorded_at: Option<DateTime<Utc>>,
}

/// Lists existing decisions for an active authenticated actor by user ID.
///
/// SECURITY: The caller must authenticate the session before passing its user
/// ID. This query also checks current database roles and memberships in the
/// same repeatable-read snapshot. Only Admin sees policy waivers; Viewer and
/// Operator see CVE environment decisions in their member environments and
/// host decisions on systems currently in those environments. A moved host's
/// historical decision is not visible to its previous environment's reader.
/// An absent or roleless actor is rejected, not treated as an empty register.
/// Rows, total, and authorization share a read-only snapshot. Offset pages on
/// separate requests do not pin a snapshot across requests.
///
/// # Errors
///
/// Returns an error for an inactive or roleless actor, invalid bounds or status,
/// or a database failure. No decision or audit rows are changed.
pub async fn list(
    pool: &PgPool,
    actor_id: Uuid,
    query: &AcceptanceListQuery,
) -> Result<AcceptancePage, AcceptanceReadError> {
    let (limit, offset) = validate(query)?;
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    let page = list_scoped_tx(&mut tx, actor_id, query, limit, offset).await?;
    tx.commit().await?;
    Ok(page)
}

/// Lists authorized decisions in the caller's repeatable-read, read-only snapshot.
///
/// SECURITY: The caller must authenticate `actor_id` and start a REPEATABLE READ
/// READ ONLY transaction before any reads. This function rechecks current roles
/// and memberships in that snapshot, alongside the rows and filtered total.
/// The caller owns the transaction and may page through the full result in the
/// same snapshot; this function does not commit or change its isolation level.
/// Each page still has the normal 1..100 limit.
///
/// # Errors
///
/// Returns an error for an inactive or roleless actor, invalid bounds or status,
/// or a database or projection failure. No decision or audit rows are changed.
pub async fn list_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor_id: Uuid,
    query: &AcceptanceListQuery,
) -> Result<AcceptancePage, AcceptanceReadError> {
    let (limit, offset) = validate(query)?;
    list_scoped_tx(tx, actor_id, query, limit, offset).await
}

fn validate(query: &AcceptanceListQuery) -> Result<(i64, i64), AcceptanceReadError> {
    let limit = query.limit.unwrap_or(25);
    let offset = query.offset.unwrap_or(0);
    if !(1..=100).contains(&limit) {
        return Err(AcceptanceReadError::Validation("limit must be 1..100"));
    }
    if offset < 0 || offset.checked_add(limit).is_none() {
        return Err(AcceptanceReadError::Validation("invalid offset"));
    }
    if !query.status.as_deref().is_none_or(|status| {
        matches!(
            status,
            "pending" | "accepted" | "rejected" | "revoked" | "expired" | "accepted_or_converted"
        )
    }) {
        return Err(AcceptanceReadError::Validation("invalid acceptance status"));
    }

    Ok((limit, offset))
}

async fn list_scoped_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor_id: Uuid,
    query: &AcceptanceListQuery,
    limit: i64,
    offset: i64,
) -> Result<AcceptancePage, AcceptanceReadError> {
    let roles: Vec<String> = sqlx::query_scalar(
        "SELECT role::text FROM user_role_assignments WHERE user_id=$1 AND EXISTS (SELECT 1 FROM users WHERE id=$1 AND is_active)",
    )
    .bind(actor_id)
    .fetch_all(&mut **tx)
    .await?;
    if !roles
        .iter()
        .any(|role| matches!(role.as_str(), "admin" | "operator" | "viewer"))
    {
        return Err(AcceptanceReadError::Forbidden);
    }
    let is_admin = roles.iter().any(|role| role == "admin");
    let environments: Vec<Uuid> = if is_admin {
        Vec::new()
    } else {
        sqlx::query_scalar(
            "SELECT environment_id FROM user_environment_memberships WHERE user_id=$1",
        )
        .bind(actor_id)
        .fetch_all(&mut **tx)
        .await?
    };
    // SECURITY: Both source selection and actor scope precede LIMIT and COUNT.
    // A host is scoped by its current membership, but its original stored scope
    // remains system_id; never label that current environment as historical.
    let rows = sqlx::query_as::<_, PageRow>(
        r#"
        WITH decisions AS (
          SELECT 'policy_waiver'::text AS source,w.id AS source_id,w.updated_at AS waiver_updated_at,w.status,
            w.finding_id,f.system_id,NULL::uuid AS environment_id,
            f.policy_lineage_id,w.policy_version_id,
            NULL::text AS canonical_cve_id,NULL::text AS canonical_package_name,
            w.justification,NULL::date AS review_date,w.review_due_at,w.expires_at,
            w.accepted_by,w.accepted_at,NULL::timestamptz AS retired_at,
            NULL::uuid AS retired_by,NULL::text AS retirement_reason,
            w.created_at AS recorded_at,replacement.poam_id AS replacement_poam_id
          FROM finding_waivers w JOIN poam_findings f ON f.id=w.finding_id
          JOIN systems waiver_host ON waiver_host.id=f.system_id
          LEFT JOIN finding_waiver_poam_replacements replacement ON replacement.waiver_id=w.id
          WHERE $2 AND ($5::uuid IS NULL OR waiver_host.environment_id=$5)
            AND ($3::text IS NULL OR $3='policy_waiver')
            AND ($4::text IS NULL OR w.status=$4
              OR ($4='accepted_or_converted' AND (w.status IN ('accepted','expired')
                OR replacement.waiver_id IS NOT NULL)))
          UNION ALL
          SELECT 'cve_host',d.id,NULL::timestamptz,d.state,NULL::uuid,d.system_id,NULL::uuid,
            NULL::uuid,NULL::uuid,d.canonical_cve_id::text,d.canonical_package_name,
             d.justification,d.review_date,NULL::date,NULL::timestamptz,d.accepted_by,
             d.accepted_at,d.retired_at,d.retired_by,d.retirement_reason,d.accepted_at,
             conversion.poam_id
          FROM cve_system_dispositions d JOIN systems s ON s.id=d.system_id
          LEFT JOIN LATERAL (
            SELECT (audit.metadata->>'poam_id')::uuid AS poam_id
            FROM admin_audit_events audit
            WHERE d.retirement_reason='converted_to_poam'
              AND audit.action='cve_acceptance_converted'
              AND audit.metadata->>'predecessor_id'=d.id::text
              AND audit.metadata->>'source_type'='host'
            ORDER BY audit.created_at DESC LIMIT 1
          ) conversion ON TRUE
          WHERE d.state='accepted' AND ($3::text IS NULL OR $3='cve_host')
            AND ($4::text IS NULL OR $4='accepted'
              OR ($4='accepted_or_converted' AND (d.retired_at IS NULL OR conversion.poam_id IS NOT NULL)))
            AND ($2 OR s.environment_id=ANY($1))
            AND ($5::uuid IS NULL OR s.environment_id=$5)
          UNION ALL
          SELECT 'cve_environment',d.id,NULL::timestamptz,d.state,NULL::uuid,NULL::uuid,d.environment_id,
            NULL::uuid,NULL::uuid,d.canonical_cve_id::text,d.canonical_package_name,
             d.justification,d.review_date,NULL::date,NULL::timestamptz,d.accepted_by,
             d.accepted_at,d.retired_at,d.retired_by,d.retirement_reason,d.accepted_at,
             conversion.poam_id
          FROM cve_environment_dispositions d
          LEFT JOIN LATERAL (
            SELECT (audit.metadata->>'poam_id')::uuid AS poam_id
            FROM admin_audit_events audit
            WHERE d.retirement_reason='converted_to_poam'
              AND audit.action='cve_acceptance_converted'
              AND audit.metadata->>'predecessor_id'=d.id::text
              AND audit.metadata->>'source_type'='environment'
            ORDER BY audit.created_at DESC LIMIT 1
          ) conversion ON TRUE
          WHERE d.state='accepted' AND ($3::text IS NULL OR $3='cve_environment')
            AND ($4::text IS NULL OR $4='accepted'
              OR ($4='accepted_or_converted' AND (d.retired_at IS NULL OR conversion.poam_id IS NOT NULL)))
            AND ($2 OR d.environment_id=ANY($1))
            AND ($5::uuid IS NULL OR d.environment_id=$5)
        ), total AS (SELECT count(*) AS total FROM decisions),
        page AS (SELECT * FROM decisions
          ORDER BY recorded_at DESC,source COLLATE "C",source_id DESC
          LIMIT $6 OFFSET $7)
        SELECT total.total,page.* FROM total LEFT JOIN page ON TRUE
        ORDER BY page.recorded_at DESC,page.source COLLATE "C",page.source_id DESC
    "#,
    )
    .bind(&environments)
    .bind(is_admin)
    .bind(query.source.map(AcceptanceSource::as_str))
    .bind(query.status.as_deref())
    .bind(query.environment_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&mut **tx)
    .await?;
    let total = rows.first().map_or(0, |row| row.total);
    let items = rows
        .into_iter()
        .filter(|row| row.source.is_some())
        .map(|row| {
            Ok(AcceptanceEntry {
                source: match row.source.as_deref() {
                    Some("policy_waiver") => AcceptanceSource::PolicyWaiver,
                    Some("cve_host") => AcceptanceSource::CveHost,
                    Some("cve_environment") => AcceptanceSource::CveEnvironment,
                    _ => return Err(anyhow!("unknown acceptance source")),
                },
                source_id: row
                    .source_id
                    .ok_or_else(|| anyhow!("decision missing ID"))?,
                waiver_updated_at: row.waiver_updated_at,
                status: row
                    .status
                    .ok_or_else(|| anyhow!("decision missing status"))?,
                finding_id: row.finding_id,
                system_id: row.system_id,
                environment_id: row.environment_id,
                policy_lineage_id: row.policy_lineage_id,
                policy_version_id: row.policy_version_id,
                canonical_cve_id: row.canonical_cve_id,
                canonical_package_name: row.canonical_package_name,
                justification: row
                    .justification
                    .ok_or_else(|| anyhow!("decision missing justification"))?,
                review_date: row.review_date,
                review_due_at: row.review_due_at,
                expires_at: row.expires_at,
                accepted_by: row.accepted_by,
                accepted_at: row.accepted_at,
                retired_at: row.retired_at,
                retired_by: row.retired_by,
                retirement_reason: row.retirement_reason,
                replacement_poam_id: row.replacement_poam_id,
                recorded_at: row
                    .recorded_at
                    .ok_or_else(|| anyhow!("decision missing timestamp"))?,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(AcceptancePage {
        items,
        total,
        limit,
        offset,
        has_more: offset < total && limit < total - offset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified disposable PG35457"]
    async fn caller_snapshot_pages_full_authorized_register(pool: PgPool) {
        let admin: Uuid = sqlx::query_scalar("INSERT INTO users(username,first_name,last_name,email) VALUES($1,'Admin','Reader',$2) RETURNING id")
            .bind(format!("snap-a-{}", Uuid::new_v4()))
            .bind(format!("snapshot-admin-{}@example.invalid", Uuid::new_v4()))
            .fetch_one(&pool).await.unwrap();
        let viewer: Uuid = sqlx::query_scalar("INSERT INTO users(username,first_name,last_name,email) VALUES($1,'Viewer','Reader',$2) RETURNING id")
            .bind(format!("snap-v-{}", Uuid::new_v4()))
            .bind(format!("snapshot-viewer-{}@example.invalid", Uuid::new_v4()))
            .fetch_one(&pool).await.unwrap();
        for (id, role) in [(admin, "admin"), (viewer, "viewer")] {
            sqlx::query("INSERT INTO user_role_assignments(user_id,role) VALUES($1,$2::auth_role)")
                .bind(id)
                .bind(role)
                .execute(&pool)
                .await
                .unwrap();
        }
        let a: Uuid = sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
            .bind(format!("snapshot-a-{}", Uuid::new_v4()))
            .fetch_one(&pool)
            .await
            .unwrap();
        let b: Uuid = sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
            .bind(format!("snapshot-b-{}", Uuid::new_v4()))
            .fetch_one(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)",
        )
        .bind(viewer)
        .bind(a)
        .execute(&pool)
        .await
        .unwrap();
        let host: Uuid = sqlx::query_scalar("INSERT INTO systems(hostname,public_key,derivation,environment_id) VALUES($1,$2,$2,$3) RETURNING id")
            .bind(format!("snap-h-{}", Uuid::new_v4()))
            .bind("snapshot-test-key").bind(a).fetch_one(&pool).await.unwrap();
        let cve = "CVE-2099-12345";
        sqlx::query("INSERT INTO cves(id) VALUES($1)")
            .bind(cve)
            .execute(&pool)
            .await
            .unwrap();
        let host_id: Uuid = sqlx::query_scalar("INSERT INTO cve_system_dispositions(canonical_cve_id,canonical_package_name,system_id,state,justification,accepted_by,accepted_at) VALUES($1,'openssl',$2,'accepted','host review',$3,now()) RETURNING id")
            .bind(cve).bind(host).bind(admin).fetch_one(&pool).await.unwrap();
        sqlx::query("INSERT INTO cve_environment_dispositions(canonical_cve_id,canonical_package_name,environment_id,state,justification,accepted_by,accepted_at) SELECT $1, 'snapshot-' || n, $2, 'accepted', 'environment review', $3, now() FROM generate_series(1, 102) n")
            .bind(cve).bind(a).bind(admin).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO cve_environment_dispositions(canonical_cve_id,canonical_package_name,environment_id,state,justification,accepted_by,accepted_at) VALUES($1,'hidden',$2,'accepted','hidden review',$3,now())")
            .bind(cve).bind(b).bind(admin).execute(&pool).await.unwrap();

        let mut tx = pool.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await
            .unwrap();
        let environment_query = AcceptanceListQuery {
            source: Some(AcceptanceSource::CveEnvironment),
            limit: Some(100),
            ..Default::default()
        };
        let first = list_tx(&mut tx, viewer, &environment_query).await.unwrap();
        assert_eq!(first.total, 102);
        assert_eq!(first.items.len(), 100);
        assert!(first.has_more);
        let last = list_tx(
            &mut tx,
            viewer,
            &AcceptanceListQuery {
                offset: Some(100),
                ..environment_query
            },
        )
        .await
        .unwrap();
        assert_eq!(last.total, 102);
        assert_eq!(last.items.len(), 2);
        assert!(!last.has_more);
        assert!(first.items.iter().chain(&last.items).all(|row| {
            row.source == AcceptanceSource::CveEnvironment && row.environment_id == Some(a)
        }));
        let host_page = list_tx(
            &mut tx,
            viewer,
            &AcceptanceListQuery {
                source: Some(AcceptanceSource::CveHost),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(host_page.total, 1);
        assert_eq!(host_page.items[0].source_id, host_id);
        assert_eq!(host_page.items[0].system_id, Some(host));
        assert_eq!(
            list_tx(
                &mut tx,
                viewer,
                &AcceptanceListQuery {
                    source: Some(AcceptanceSource::CveHost),
                    environment_id: Some(b),
                    ..Default::default()
                }
            )
            .await
            .unwrap()
            .total,
            0
        );
        let combined = list_tx(&mut tx, viewer, &AcceptanceListQuery::default())
            .await
            .unwrap();
        assert_eq!(combined.total, 103);
        assert_eq!(
            list_tx(&mut tx, admin, &AcceptanceListQuery::default())
                .await
                .unwrap()
                .total,
            104
        );

        sqlx::query("INSERT INTO cve_environment_dispositions(canonical_cve_id,canonical_package_name,environment_id,state,justification,accepted_by,accepted_at) VALUES($1,'later',$2,'accepted','later review',$3,now())")
            .bind(cve).bind(a).bind(admin).execute(&pool).await.unwrap();
        sqlx::query("DELETE FROM user_environment_memberships WHERE user_id=$1")
            .bind(viewer)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE users SET is_active=false WHERE id=$1")
            .bind(viewer)
            .execute(&pool)
            .await
            .unwrap();
        let unchanged = list_tx(&mut tx, viewer, &AcceptanceListQuery::default())
            .await
            .unwrap();
        assert_eq!(unchanged.total, 103);
        assert_eq!(unchanged.items[0].source_id, combined.items[0].source_id);
        assert!(matches!(
            list(&pool, viewer, &AcceptanceListQuery::default()).await,
            Err(AcceptanceReadError::Forbidden)
        ));
        tx.commit().await.unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified disposable PG35457"]
    async fn accepted_decisions_are_scoped_before_pagination(pool: PgPool) {
        let admin: Uuid = sqlx::query_scalar("INSERT INTO users(username,first_name,last_name,email) VALUES($1,'Admin','Reader',$2) RETURNING id")
            .bind(format!("accept-admin-{}", Uuid::new_v4()))
            .bind(format!("accept-admin-{}@example.invalid", Uuid::new_v4()))
            .fetch_one(&pool).await.unwrap();
        let viewer: Uuid = sqlx::query_scalar("INSERT INTO users(username,first_name,last_name,email) VALUES($1,'Viewer','Reader',$2) RETURNING id")
            .bind(format!("accept-viewer-{}", Uuid::new_v4()))
            .bind(format!("accept-viewer-{}@example.invalid", Uuid::new_v4()))
            .fetch_one(&pool).await.unwrap();
        for (id, role) in [(admin, "admin"), (viewer, "viewer")] {
            sqlx::query("INSERT INTO user_role_assignments(user_id,role) VALUES($1,$2::auth_role)")
                .bind(id)
                .bind(role)
                .execute(&pool)
                .await
                .unwrap();
        }
        let a: Uuid = sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
            .bind(format!("accept-a-{}", Uuid::new_v4()))
            .fetch_one(&pool)
            .await
            .unwrap();
        let b: Uuid = sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
            .bind(format!("accept-b-{}", Uuid::new_v4()))
            .fetch_one(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)",
        )
        .bind(viewer)
        .bind(a)
        .execute(&pool)
        .await
        .unwrap();
        let host: Uuid = sqlx::query_scalar(
            "INSERT INTO systems(hostname,public_key,derivation,environment_id) VALUES($1,$2,$2,$3) RETURNING id",
        ).bind(format!("accept-host-{}", Uuid::new_v4()))
            .bind("test-accept-key").bind(a).fetch_one(&pool).await.unwrap();
        let cve = "CVE-2099-12345";
        sqlx::query("INSERT INTO cves(id) VALUES($1)")
            .bind(cve)
            .execute(&pool)
            .await
            .unwrap();
        let host_id: Uuid = sqlx::query_scalar(
            "INSERT INTO cve_system_dispositions(canonical_cve_id,canonical_package_name,system_id,state,justification,accepted_by,accepted_at) VALUES($1,'openssl',$2,'accepted','host review',$3,now()) RETURNING id",
        ).bind(cve).bind(host).bind(admin).fetch_one(&pool).await.unwrap();
        let env_id: Uuid = sqlx::query_scalar(
            "INSERT INTO cve_environment_dispositions(canonical_cve_id,canonical_package_name,environment_id,state,justification,accepted_by,accepted_at) VALUES($1,'openssl',$2,'accepted','environment review',$3,now()) RETURNING id",
        ).bind(cve).bind(a).bind(admin).fetch_one(&pool).await.unwrap();
        sqlx::query("INSERT INTO cve_environment_dispositions(canonical_cve_id,canonical_package_name,environment_id,state,justification,accepted_by,accepted_at) VALUES($1,'openssl',$2,'accepted','hidden review',$3,now())")
            .bind(cve).bind(b).bind(admin).execute(&pool).await.unwrap();
        let page = list(
            &pool,
            viewer,
            &AcceptanceListQuery {
                limit: Some(1),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.items.len(), 1);
        assert!(page.has_more);
        let next = list(
            &pool,
            viewer,
            &AcceptanceListQuery {
                limit: Some(1),
                offset: Some(1),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(next.total, 2);
        assert!(!next.has_more);
        assert_eq!(
            vec![page.items[0].source_id, next.items[0].source_id],
            vec![env_id, host_id]
        );
        assert_eq!(next.items[0].system_id, Some(host));
        assert_eq!(next.items[0].environment_id, None);
        assert!(next.items[0].expires_at.is_none());
        assert!(next.items[0].review_date.is_none());
        let admin_page = list(&pool, admin, &AcceptanceListQuery::default())
            .await
            .unwrap();
        assert_eq!(admin_page.total, 3);
        let policy: Uuid = sqlx::query_scalar(
            "INSERT INTO deployment_policies(name,policy_type,config,enabled) VALUES($1,'custom_check','{}',false) RETURNING id",
        )
        .bind(format!("accept-policy-{}", Uuid::new_v4()))
        .fetch_one(&pool).await.unwrap();
        let finding: Uuid = sqlx::query_scalar(
            "INSERT INTO poam_findings(system_id,policy_lineage_id) VALUES($1,$2) RETURNING id",
        )
        .bind(host)
        .bind(policy)
        .fetch_one(&pool)
        .await
        .unwrap();
        let waiver: Uuid = sqlx::query_scalar(
            "INSERT INTO finding_waivers(finding_id,justification,policy_version_id,observation_token,observation_snapshot,created_by) VALUES($1,'Approved only after review',$2,'test-observation','{}',$3) RETURNING id",
        ).bind(finding).bind(Uuid::new_v4()).bind(admin).fetch_one(&pool).await.unwrap();
        sqlx::query("UPDATE finding_waivers SET status='accepted',accepted_by=$1,accepted_at=now(),expires_at=now()+interval '60 days' WHERE id=$2")
            .bind(admin).bind(waiver).execute(&pool).await.unwrap();
        let policy_page = list(
            &pool,
            admin,
            &AcceptanceListQuery {
                source: Some(AcceptanceSource::PolicyWaiver),
                status: Some("accepted".into()),
                environment_id: Some(a),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(policy_page.total, 1);
        assert_eq!(policy_page.items[0].source_id, waiver);
        assert_eq!(policy_page.items[0].system_id, Some(host));
        assert!(policy_page.items[0].review_date.is_none());
        assert!(policy_page.items[0].expires_at.is_some());
        assert_eq!(
            list(&pool, viewer, &AcceptanceListQuery::default())
                .await
                .unwrap()
                .total,
            2
        );
        sqlx::query("UPDATE systems SET environment_id=$1 WHERE id=$2")
            .bind(b)
            .bind(host)
            .execute(&pool)
            .await
            .unwrap();
        let moved = list(&pool, viewer, &AcceptanceListQuery::default())
            .await
            .unwrap();
        assert_eq!(moved.total, 1);
        assert_eq!(moved.items[0].source_id, env_id);
        let after_move = list(
            &pool,
            admin,
            &AcceptanceListQuery {
                source: Some(AcceptanceSource::PolicyWaiver),
                environment_id: Some(a),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(after_move.items.is_empty());

        // Pagination applies after scope and source filtering, not before it.
        for index in 0..102 {
            sqlx::query(
                "INSERT INTO cve_environment_dispositions(canonical_cve_id,canonical_package_name,environment_id,state,justification,accepted_by,accepted_at) VALUES($1,$2,$3,'accepted','paged decision',$4,now())",
            )
            .bind(cve)
            .bind(format!("paged-{index}"))
            .bind(a)
            .bind(admin)
            .execute(&pool)
            .await
            .unwrap();
        }
        let source_page = AcceptanceListQuery {
            source: Some(AcceptanceSource::CveEnvironment),
            environment_id: Some(a),
            limit: Some(100),
            ..Default::default()
        };
        let first = list(&pool, viewer, &source_page).await.unwrap();
        assert_eq!(first.total, 103);
        assert_eq!(first.items.len(), 100);
        assert!(first.has_more);
        let last = list(
            &pool,
            viewer,
            &AcceptanceListQuery {
                offset: Some(100),
                ..source_page
            },
        )
        .await
        .unwrap();
        assert_eq!(last.total, 103);
        assert_eq!(last.items.len(), 3);
        assert!(!last.has_more);
        sqlx::query("UPDATE users SET is_active=false WHERE id=$1")
            .bind(viewer)
            .execute(&pool)
            .await
            .unwrap();
        assert!(matches!(
            list(&pool, viewer, &AcceptanceListQuery::default()).await,
            Err(AcceptanceReadError::Forbidden)
        ));
    }
}
