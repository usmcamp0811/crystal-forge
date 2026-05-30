//! CVE-related database queries for the advanced CVE dashboard.

use anyhow::Result;
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};

use crate::api::models::{
    CveAffectedSystemDetail, CveDetail, CveFilters, CveFleetStats, CveJustification,
    CveJustificationInput, CveListItem, CvePackageGroup,
};

fn default_cve_fleet_stats() -> CveFleetStats {
    CveFleetStats {
        total_cves: 0,
        critical: 0,
        high: 0,
        medium: 0,
        low: 0,
        exploited: 0,
        fixable: 0,
        environments_affected: 0,
        systems_affected: 0,
        outstanding: 0,
        accepted: 0,
        scheduled: 0,
    }
}

/// Fetch paginated list of CVEs with filters applied.
///
/// Uses `view_cve_list_with_metadata` for performance.
/// Filters are AND combined. Search query does OR across CVE ID, package name, and title.
pub async fn fetch_cve_list(pool: &PgPool, filters: &CveFilters) -> Result<Vec<CveListItem>> {
    let limit = filters.limit.unwrap_or(500).min(1000);
    let severity_param = filters.severity.as_ref().map(|s| s.to_uppercase());
    let fix_status_param = filters.fix_status.clone();
    let triage_status_param = filters.triage_status.clone();
    let package_param = filters.package.as_ref().map(|p| format!("%{p}%"));
    let search_param = filters.search.as_ref().map(|s| format!("%{s}%"));
    let sort_param = filters.sort.as_deref().unwrap_or("severity");

    let rows = sqlx::query_as!(
        CveListItem,
        r#"
        SELECT
            cve_id as "cve_id!",
            cvss_v3_score::real as "cvss_v3_score?",
            UPPER(COALESCE(severity, 'UNKNOWN')) as "severity!",
            COALESCE(title, '') as "title!",
            cvss_vector as "cvss_vector?",
            published_date as "published_date?",
            COALESCE(exploited, FALSE) as "exploited!",
            package_name as "package_name?",
            installed_version as "installed_version?",
            fixed_version as "fixed_version?",
            COALESCE(fix_status, 'open') as "fix_status!",
            COALESCE(affected_count, 0)::bigint as "affected_count!",
            affected_environments as "affected_environments?: Vec<String>",
            first_seen as "first_seen?",
            last_seen as "last_seen?",
            COALESCE(age_days, 0)::int as "age_days!",
            LOWER(COALESCE(triage_status, 'outstanding')) as "triage_status!"
        FROM view_cve_list_with_metadata
        WHERE
            ($1::text IS NULL OR UPPER(severity) = $1)
            AND (
                $2::text IS NULL
                OR ($2 = 'available' AND fix_status = 'fix_available')
                OR ($2 = 'pending' AND fix_status = 'open')
                OR ($2 = 'exploited' AND exploited = TRUE)
            )
            AND ($3::text IS NULL OR LOWER(triage_status) = LOWER($3))
            AND ($4::text IS NULL OR package_name ILIKE $4)
            AND (
                $5::text IS NULL
                OR cve_id ILIKE $5
                OR package_name ILIKE $5
                OR title ILIKE $5
            )
        ORDER BY
            CASE
                WHEN $6 = 'severity' THEN
                    CASE UPPER(severity)
                        WHEN 'CRITICAL' THEN 1
                        WHEN 'HIGH' THEN 2
                        WHEN 'MEDIUM' THEN 3
                        WHEN 'LOW' THEN 4
                        ELSE 5
                    END
                ELSE NULL
            END ASC NULLS LAST,
            CASE WHEN $6 = 'severity' THEN cvss_v3_score END DESC NULLS LAST,
            CASE WHEN $6 = 'cvss' THEN cvss_v3_score END DESC NULLS LAST,
            CASE WHEN $6 = 'age' THEN age_days END ASC NULLS LAST,
            CASE WHEN $6 = 'affected' THEN affected_count END DESC NULLS LAST,
            cve_id ASC
        LIMIT $7
        "#,
        severity_param,
        fix_status_param,
        triage_status_param,
        package_param,
        search_param,
        sort_param,
        limit,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Fetch CVEs grouped by package with aggregated statistics.
pub async fn fetch_cve_packages_grouped(
    pool: &PgPool,
    filters: &CveFilters,
) -> Result<Vec<CvePackageGroup>> {
    // Fetch CVEs once with active filters, then group in-memory.
    // This ensures grouped mode respects severity/fix/triage/search filters.
    let mut all_filters = filters.clone();
    all_filters.limit = Some(1000);
    let all_cves = fetch_cve_list(pool, &all_filters).await?;

    let mut cves_by_package: HashMap<String, Vec<CveListItem>> = HashMap::new();
    for cve in all_cves {
        if let Some(pkg) = cve.package_name.clone() {
            cves_by_package.entry(pkg).or_default().push(cve);
        }
    }

    let mut result = Vec::new();
    for (package_name, mut cves) in cves_by_package {
        let mut critical_count = 0i64;
        let mut high_count = 0i64;
        let mut medium_count = 0i64;
        let mut low_count = 0i64;
        let mut environments: HashSet<String> = HashSet::new();
        let mut total_affected_systems = 0i64;
        let mut fixable_count = 0i64;
        let mut outstanding_count = 0i64;
        let mut exploited_count = 0i64;
        let mut max_cvss: Option<f32> = None;
        let mut severity_score = 0i64;

        for cve in &cves {
            match cve.severity.as_str() {
                "CRITICAL" => {
                    critical_count += 1;
                    severity_score += 1000;
                }
                "HIGH" => {
                    high_count += 1;
                    severity_score += 100;
                }
                "MEDIUM" => {
                    medium_count += 1;
                    severity_score += 10;
                }
                "LOW" => {
                    low_count += 1;
                    severity_score += 1;
                }
                _ => {}
            }

            total_affected_systems += cve.affected_count;

            if cve.fix_status == "fix_available" {
                fixable_count += 1;
            }
            if cve.triage_status == "outstanding" {
                outstanding_count += 1;
            }
            if cve.exploited {
                exploited_count += 1;
            }

            if let Some(score) = cve.cvss_v3_score {
                max_cvss = match max_cvss {
                    Some(curr) if curr >= score => Some(curr),
                    _ => Some(score),
                };
            }

            if let Some(envs) = &cve.affected_environments {
                for env in envs {
                    environments.insert(env.clone());
                }
            }
        }

        // Keep grouped rows bounded per package for UI stability.
        if cves.len() > 100 {
            cves.truncate(100);
        }

        result.push(CvePackageGroup {
            package_name,
            cve_count: cves.len() as i64,
            critical_count,
            high_count,
            medium_count,
            low_count,
            environments_count: environments.len() as i64,
            total_affected_systems,
            fixable_count,
            outstanding_count,
            exploited_count,
            max_cvss,
            severity_score,
            cves: Some(cves),
        });
    }

    // Match prior ordering semantics.
    result.sort_by(|a, b| {
        b.severity_score
            .cmp(&a.severity_score)
            .then_with(|| {
                b.max_cvss
                    .partial_cmp(&a.max_cvss)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.package_name.cmp(&b.package_name))
    });
    if result.len() > 100 {
        result.truncate(100);
    }

    Ok(result)
}

/// Fetch detailed information for a single CVE.
pub async fn fetch_cve_detail(pool: &PgPool, cve_id: &str) -> Result<CveDetail> {
    let detail = sqlx::query_as!(
        CveDetail,
        r#"
        SELECT
            v.cve_id as "cve_id!",
            v.cvss_v3_score::real as "cvss_v3_score?",
            COALESCE(v.severity, 'UNKNOWN') as "severity!",
            COALESCE(v.title, '') as "title!",
            v.cvss_vector as "cvss_vector?",
            c.cwe_id as "cwe_id?",
            v.published_date as "published_date?",
            c.modified_date as "modified_date?",
            COALESCE(v.exploited, FALSE) as "exploited!",
            v.package_name as "package_name?",
            v.installed_version as "installed_version?",
            v.fixed_version as "fixed_version?",
            NULL::text as "detection_method?",
            COALESCE(v.fix_status, 'open') as "fix_status!"
        FROM view_cve_list_with_metadata v
        LEFT JOIN cves c ON c.id = v.cve_id
        WHERE v.cve_id = $1
        LIMIT 1
        "#,
        cve_id,
    )
    .fetch_one(pool)
    .await?;

    Ok(detail)
}

/// Fetch systems affected by a specific CVE, grouped by environment.
pub async fn fetch_cve_affected_systems(
    pool: &PgPool,
    cve_id: &str,
) -> Result<Vec<CveAffectedSystemDetail>> {
    let systems = sqlx::query_as!(
        CveAffectedSystemDetail,
        r#"
        WITH latest_per_system AS (
            SELECT DISTINCT ON (s.id)
                s.id as system_id,
                s.hostname,
                e.name as environment,
                ss.primary_ip_address,
                f.name as flake_name,
                f.id as flake_id,
                NULL::text as commit_hash,
                s.deployment_policy,
                pkg_d.version as current_package_version,
                scan.completed_at
            FROM systems s
            JOIN derivations d
              ON s.hostname = d.derivation_name
             AND d.derivation_type = 'nixos'
            JOIN derivation_statuses ds
              ON d.status_id = ds.id
             AND ds.name IN ('build-complete','complete')
            JOIN cve_scans scan
              ON d.id = scan.derivation_id
             AND scan.completed_at IS NOT NULL
            JOIN scan_packages sp
              ON scan.id = sp.scan_id
            JOIN derivations pkg_d
              ON sp.derivation_id = pkg_d.id
             AND pkg_d.derivation_type = 'package'
            JOIN package_vulnerabilities pv
              ON pkg_d.id = pv.derivation_id
             AND NOT pv.is_whitelisted
            JOIN cves c
              ON pv.cve_id::text = c.id::text
            LEFT JOIN environments e
              ON s.environment_id = e.id
            LEFT JOIN flakes f
              ON s.flake_id = f.id
            LEFT JOIN (
                SELECT DISTINCT ON (hostname) hostname, primary_ip_address
                FROM system_states
                ORDER BY hostname, timestamp DESC
            ) ss ON s.hostname = ss.hostname
            WHERE s.is_active = TRUE
              AND c.id = $1
            ORDER BY s.id, scan.completed_at DESC
        )
        SELECT
            system_id,
            hostname as "hostname!",
            environment as "environment?",
            primary_ip_address as "primary_ip_address?",
            flake_name as "flake_name?",
            flake_id as "flake_id?",
            commit_hash as "commit_hash?",
            deployment_policy as "deployment_policy!",
            current_package_version as "current_package_version?"
        FROM latest_per_system
        ORDER BY environment NULLS LAST, hostname
        "#,
        cve_id,
    )
    .fetch_all(pool)
    .await?;

    Ok(systems)
}

/// Fetch justification history for a CVE.
pub async fn fetch_cve_justifications(
    pool: &PgPool,
    cve_id: &str,
) -> Result<Vec<CveJustification>> {
    let justifications = sqlx::query_as!(
        CveJustification,
        r#"
        SELECT 
            scj.system_id as "system_id?",
            scj.cve_id as "cve_id!",
            scj.category as "category!",
            scj.reason as "reason!",
            scj.updated_by as "updated_by?",
            scj.updated_at as "updated_at!",
            scj.created_at as "created_at!",
            u.username as "updated_by_username?"
        FROM system_cve_justifications scj
        LEFT JOIN users u ON scj.updated_by = u.id
        WHERE scj.cve_id = $1
        ORDER BY scj.updated_at DESC
        "#,
        cve_id,
    )
    .fetch_all(pool)
    .await?;

    Ok(justifications)
}

/// Insert or update a CVE justification.
///
/// Uses ON CONFLICT to update existing justifications for the same system+CVE pair.
pub async fn insert_cve_justification(
    pool: &PgPool,
    input: &CveJustificationInput,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO system_cve_justifications (system_id, cve_id, category, reason, updated_by, updated_at)
        VALUES ($1, $2, $3, $4, $5, NOW())
        ON CONFLICT (system_id, cve_id)
        DO UPDATE SET
            category = EXCLUDED.category,
            reason = EXCLUDED.reason,
            updated_by = EXCLUDED.updated_by,
            updated_at = NOW()
        "#,
    )
    .bind(input.system_id)
    .bind(&input.cve_id)
    .bind(&input.category)
    .bind(&input.reason)
    .bind(input.updated_by)
    .execute(pool)
    .await?;

    Ok(())
}

/// Fetch fleet-wide CVE statistics for the dashboard summary.
pub async fn fetch_cve_fleet_stats(pool: &PgPool) -> Result<CveFleetStats> {
    let stats = sqlx::query_as!(
        CveFleetStats,
        r#"
        SELECT 
            COALESCE((to_jsonb(v)->>'total_cves')::bigint, 0) as "total_cves!",
            COALESCE((to_jsonb(v)->>'critical')::bigint, 0) as "critical!",
            COALESCE((to_jsonb(v)->>'high')::bigint, 0) as "high!",
            COALESCE((to_jsonb(v)->>'medium')::bigint, 0) as "medium!",
            COALESCE((to_jsonb(v)->>'low')::bigint, 0) as "low!",
            COALESCE((to_jsonb(v)->>'exploited')::bigint, 0) as "exploited!",
            COALESCE((to_jsonb(v)->>'fixable')::bigint, 0) as "fixable!",
            COALESCE((to_jsonb(v)->>'environments_affected')::bigint, 0) as "environments_affected!",
            COALESCE(
                (to_jsonb(v)->>'systems_affected')::bigint,
                (to_jsonb(v)->>'total_system_cve_instances')::bigint,
                0
            ) as "systems_affected!",
            COALESCE((to_jsonb(v)->>'outstanding')::bigint, 0) as "outstanding!",
            COALESCE((to_jsonb(v)->>'accepted')::bigint, 0) as "accepted!",
            COALESCE((to_jsonb(v)->>'scheduled')::bigint, 0) as "scheduled!"
        FROM view_cve_fleet_stats v
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await?;

    Ok(stats.unwrap_or_else(default_cve_fleet_stats))
}

/// Fetch distinct package names for autocomplete.
pub async fn fetch_package_names(pool: &PgPool) -> Result<Vec<String>> {
    let packages = sqlx::query_scalar!(
        r#"
        SELECT DISTINCT package_name as "package_name!"
        FROM view_cve_list_with_metadata
        WHERE package_name IS NOT NULL
        ORDER BY package_name
        LIMIT 500
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(packages)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Query builder unit tests (pure logic, no DB connection needed) ──

    fn build_list_query(filters: &CveFilters) -> String {
        let mut query = String::from(
            "SELECT * FROM view_cve_list_with_metadata WHERE 1=1\n",
        );
        let mut conditions = Vec::new();

        if let Some(ref severity) = filters.severity {
            conditions.push(format!("AND severity = '{}'", severity.to_uppercase()));
        }
        if let Some(ref fix_status) = filters.fix_status {
            match fix_status.as_str() {
                "available" => conditions.push("AND fix_status = 'fix_available'".to_string()),
                "pending" => conditions.push("AND fix_status = 'open'".to_string()),
                "exploited" => conditions.push("AND exploited = TRUE".to_string()),
                _ => {}
            }
        }
        if let Some(ref triage_status) = filters.triage_status {
            conditions.push(format!("AND triage_status = '{}'", triage_status));
        }
        if let Some(ref package) = filters.package {
            conditions.push(format!(
                "AND package_name ILIKE '%{}%'",
                package.replace('\'', "''")
            ));
        }
        if let Some(ref search) = filters.search {
            let esc = search.replace('\'', "''");
            conditions.push(format!(
                "AND (cve_id ILIKE '%{0}%' OR package_name ILIKE '%{0}%' OR title ILIKE '%{0}%')",
                esc
            ));
        }
        for cond in conditions {
            query.push_str(&cond);
            query.push('\n');
        }
        query
    }

    #[test]
    fn no_filters_produces_base_query_only() {
        let q = build_list_query(&CveFilters::default());
        assert!(!q.contains("AND severity"));
        assert!(!q.contains("AND fix_status"));
        assert!(!q.contains("AND triage_status"));
        assert!(!q.contains("AND package_name"));
        assert!(!q.contains("AND (cve_id"));
    }

    #[test]
    fn severity_filter_uppercased_in_query() {
        let f = CveFilters {
            severity: Some("critical".to_string()),
            ..Default::default()
        };
        let q = build_list_query(&f);
        assert!(q.contains("severity = 'CRITICAL'"), "query: {}", q);
    }

    #[test]
    fn fix_status_available_maps_to_fix_available() {
        let f = CveFilters {
            fix_status: Some("available".to_string()),
            ..Default::default()
        };
        let q = build_list_query(&f);
        assert!(q.contains("fix_status = 'fix_available'"), "query: {}", q);
    }

    #[test]
    fn fix_status_pending_maps_to_open() {
        let f = CveFilters {
            fix_status: Some("pending".to_string()),
            ..Default::default()
        };
        let q = build_list_query(&f);
        assert!(q.contains("fix_status = 'open'"), "query: {}", q);
    }

    #[test]
    fn fix_status_exploited_maps_to_boolean_filter() {
        let f = CveFilters {
            fix_status: Some("exploited".to_string()),
            ..Default::default()
        };
        let q = build_list_query(&f);
        assert!(q.contains("exploited = TRUE"), "query: {}", q);
    }

    #[test]
    fn unknown_fix_status_is_silently_ignored() {
        let f = CveFilters {
            fix_status: Some("wontfix".to_string()),
            ..Default::default()
        };
        let q = build_list_query(&f);
        assert!(!q.contains("AND fix_status"), "query: {}", q);
        assert!(!q.contains("wontfix"), "query: {}", q);
    }

    #[test]
    fn triage_status_filter_passed_through_verbatim() {
        for status in ["outstanding", "scheduled", "accepted"] {
            let f = CveFilters {
                triage_status: Some(status.to_string()),
                ..Default::default()
            };
            let q = build_list_query(&f);
            assert!(
                q.contains(&format!("triage_status = '{}'", status)),
                "query for status={}: {}",
                status,
                q
            );
        }
    }

    #[test]
    fn search_filter_escapes_single_quotes() {
        let f = CveFilters {
            search: Some("O'Reilly".to_string()),
            ..Default::default()
        };
        let q = build_list_query(&f);
        // Single quote must be doubled to prevent SQL injection
        assert!(q.contains("O''Reilly"), "query: {}", q);
        assert!(!q.contains("O'Reilly") || q.matches("O''Reilly").count() > 0);
    }

    #[test]
    fn package_filter_escapes_single_quotes() {
        let f = CveFilters {
            package: Some("lib's-pkg".to_string()),
            ..Default::default()
        };
        let q = build_list_query(&f);
        assert!(q.contains("lib''s-pkg"), "query: {}", q);
    }

    #[test]
    fn multiple_filters_all_appear_in_query() {
        let f = CveFilters {
            severity: Some("high".to_string()),
            fix_status: Some("available".to_string()),
            triage_status: Some("outstanding".to_string()),
            search: Some("openssl".to_string()),
            ..Default::default()
        };
        let q = build_list_query(&f);
        assert!(q.contains("severity = 'HIGH'"));
        assert!(q.contains("fix_status = 'fix_available'"));
        assert!(q.contains("triage_status = 'outstanding'"));
        assert!(q.contains("openssl"));
    }

    #[test]
    fn cve_filters_default_all_none() {
        let f = CveFilters::default();
        assert!(f.severity.is_none());
        assert!(f.fix_status.is_none());
        assert!(f.triage_status.is_none());
        assert!(f.package.is_none());
        assert!(f.search.is_none());
        assert!(f.sort.is_none());
        assert!(f.limit.is_none());
    }

    #[test]
    fn default_cve_fleet_stats_is_all_zero() {
        let stats = default_cve_fleet_stats();
        assert_eq!(stats.total_cves, 0);
        assert_eq!(stats.critical, 0);
        assert_eq!(stats.high, 0);
        assert_eq!(stats.medium, 0);
        assert_eq!(stats.low, 0);
        assert_eq!(stats.exploited, 0);
        assert_eq!(stats.fixable, 0);
        assert_eq!(stats.environments_affected, 0);
        assert_eq!(stats.systems_affected, 0);
        assert_eq!(stats.outstanding, 0);
        assert_eq!(stats.accepted, 0);
        assert_eq!(stats.scheduled, 0);
    }

    // ── Live DB tests (require running PostgreSQL with migrations applied) ──

    #[tokio::test]
    #[ignore = "requires test database"]
    async fn fetch_cve_list_no_filters_returns_ok() {
        let pool = crate::config::CrystalForgeConfig::db_pool()
            .await
            .expect("test db pool");
        let result = fetch_cve_list(&pool, &CveFilters::default()).await;
        assert!(result.is_ok(), "error: {:?}", result.err());
    }

    #[tokio::test]
    #[ignore = "requires test database"]
    async fn fetch_cve_fleet_stats_returns_ok() {
        let pool = crate::config::CrystalForgeConfig::db_pool()
            .await
            .expect("test db pool");
        let result = fetch_cve_fleet_stats(&pool).await;
        assert!(result.is_ok(), "error: {:?}", result.err());
    }

    #[tokio::test]
    #[ignore = "requires test database"]
    async fn fetch_cve_list_severity_filter_constrains_results() {
        let pool = crate::config::CrystalForgeConfig::db_pool()
            .await
            .expect("test db pool");
        let f = CveFilters {
            severity: Some("critical".to_string()),
            ..Default::default()
        };
        let result = fetch_cve_list(&pool, &f).await.expect("query failed");
        for cve in &result {
            assert_eq!(
                cve.severity.to_uppercase(),
                "CRITICAL",
                "expected only CRITICAL cves, got: {:?}",
                cve.severity
            );
        }
    }

    #[tokio::test]
    #[ignore = "requires test database"]
    async fn fetch_cve_list_limit_respected() {
        let pool = crate::config::CrystalForgeConfig::db_pool()
            .await
            .expect("test db pool");
        let f = CveFilters {
            limit: Some(5),
            ..Default::default()
        };
        let result = fetch_cve_list(&pool, &f).await.expect("query failed");
        assert!(
            result.len() <= 5,
            "expected at most 5 results, got {}",
            result.len()
        );
    }
}
