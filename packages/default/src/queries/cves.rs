//! CVE-related database queries for the advanced CVE dashboard.

use anyhow::Result;
use sqlx::PgPool;

use crate::api::models::{
    CveAffectedSystemDetail, CveDetail, CveFilters, CveFleetStats, CveJustification,
    CveJustificationInput, CveListItem, CvePackageGroup,
};

/// Fetch paginated list of CVEs with filters applied.
///
/// Uses `view_cve_list_with_metadata` for performance.
/// Filters are AND combined. Search query does OR across CVE ID, package name, and title.
pub async fn fetch_cve_list(pool: &PgPool, filters: &CveFilters) -> Result<Vec<CveListItem>> {
    let mut query = String::from(
        r#"
        SELECT 
            cve_id,
            cvss_v3_score::real as cvss_v3_score,
            severity,
            title,
            cvss_vector,
            published_date,
            exploited,
            package_name,
            installed_version,
            fixed_version,
            fix_status,
            affected_count,
            affected_environments,
            first_seen,
            last_seen,
            age_days,
            triage_status
        FROM view_cve_list_with_metadata
        WHERE 1=1
        "#,
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
        let search_escaped = search.replace('\'', "''");
        conditions.push(format!(
            "AND (cve_id ILIKE '%{0}%' OR package_name ILIKE '%{0}%' OR title ILIKE '%{0}%')",
            search_escaped
        ));
    }

    for condition in conditions {
        query.push_str(&condition);
        query.push('\n');
    }

    // Sorting
    match filters.sort.as_deref() {
        Some("cvss") => query.push_str("ORDER BY cvss_v3_score DESC NULLS LAST"),
        Some("age") => query.push_str("ORDER BY age_days ASC"),
        Some("affected") => query.push_str("ORDER BY affected_count DESC"),
        Some("severity") | _ => {
            // Default: severity score then CVSS
            query.push_str(
                r#"ORDER BY 
                    CASE severity
                        WHEN 'CRITICAL' THEN 1
                        WHEN 'HIGH' THEN 2
                        WHEN 'MEDIUM' THEN 3
                        WHEN 'LOW' THEN 4
                        ELSE 5
                    END,
                    cvss_v3_score DESC NULLS LAST"#,
            );
        }
    }

    let limit = filters.limit.unwrap_or(500).min(1000);
    query.push_str(&format!("\nLIMIT {}", limit));

    let rows = sqlx::query_as::<_, CveListItem>(&query)
        .fetch_all(pool)
        .await?;

    Ok(rows)
}

/// Fetch CVEs grouped by package with aggregated statistics.
pub async fn fetch_cve_packages_grouped(
    pool: &PgPool,
    filters: &CveFilters,
) -> Result<Vec<CvePackageGroup>> {
    // First get package-level stats
    let package_stats = sqlx::query_as::<_, CvePackageGroup>(
        r#"
        SELECT 
            package_name,
            cve_count,
            critical_count,
            high_count,
            medium_count,
            low_count,
            environments_count,
            total_affected_systems,
            fixable_count,
            outstanding_count,
            exploited_count,
            max_cvss::real as max_cvss,
            severity_score
        FROM view_cves_grouped_by_package
        ORDER BY severity_score DESC, max_cvss DESC NULLS LAST
        LIMIT 100
        "#,
    )
    .fetch_all(pool)
    .await?;

    // For each package, fetch its CVEs (applying same filters as list view)
    let mut result = Vec::new();
    for mut pkg_group in package_stats {
        let mut pkg_filters = filters.clone();
        pkg_filters.package = Some(pkg_group.package_name.clone());
        pkg_filters.limit = Some(100); // Limit CVEs per package

        let cves = fetch_cve_list(pool, &pkg_filters).await?;
        pkg_group.cves = Some(cves);
        result.push(pkg_group);
    }

    Ok(result)
}

/// Fetch detailed information for a single CVE.
pub async fn fetch_cve_detail(pool: &PgPool, cve_id: &str) -> Result<CveDetail> {
    let detail = sqlx::query_as::<_, CveDetail>(
        r#"
        SELECT 
            c.id as cve_id,
            c.cvss_v3_score::real as cvss_v3_score,
            severity_from_cvss(c.cvss_v3_score) as severity,
            c.description as title,
            c.vector as cvss_vector,
            c.cwe_id,
            c.published_date,
            c.modified_date,
            c.exploited,
            np.pname as package_name,
            np.version as installed_version,
            pv.fixed_version,
            pv.detection_method,
            CASE WHEN pv.fixed_version IS NOT NULL THEN 'fix_available' ELSE 'open' END as fix_status
        FROM cves c
        LEFT JOIN package_vulnerabilities pv ON c.id = pv.cve_id AND NOT pv.is_whitelisted
        LEFT JOIN nix_packages np ON pv.derivation_path = np.derivation_path
        WHERE c.id = $1
        LIMIT 1
        "#,
    )
    .bind(cve_id)
    .fetch_one(pool)
    .await?;

    Ok(detail)
}

/// Fetch systems affected by a specific CVE, grouped by environment.
pub async fn fetch_cve_affected_systems(
    pool: &PgPool,
    cve_id: &str,
) -> Result<Vec<CveAffectedSystemDetail>> {
    let systems = sqlx::query_as::<_, CveAffectedSystemDetail>(
        r#"
        SELECT DISTINCT
            s.id as system_id,
            s.hostname,
            e.name as environment,
            ss.primary_ip_address,
            f.name as flake_name,
            f.id as flake_id,
            com.git_commit_hash as commit_hash,
            s.deployment_policy,
            np.version as current_package_version
        FROM cves c
        JOIN package_vulnerabilities pv ON c.id = pv.cve_id AND NOT pv.is_whitelisted
        JOIN nix_packages np ON pv.derivation_path = np.derivation_path
        JOIN scan_packages sp ON np.derivation_path = sp.derivation_path
        JOIN cve_scans cs ON sp.scan_id = cs.id AND cs.completed_at IS NOT NULL
        JOIN evaluation_targets et ON cs.evaluation_target_id = et.id
        JOIN systems s ON et.target_name = s.hostname AND s.is_active = TRUE
        LEFT JOIN environments e ON s.environment_id = e.id
        LEFT JOIN flakes f ON s.flake_id = f.id
        LEFT JOIN commits com ON et.commit_id = com.id
        LEFT JOIN (
            SELECT DISTINCT ON (hostname) hostname, primary_ip_address
            FROM system_states
            ORDER BY hostname, timestamp DESC
        ) ss ON s.hostname = ss.hostname
        WHERE c.id = $1
        ORDER BY e.name NULLS LAST, s.hostname
        "#,
    )
    .bind(cve_id)
    .fetch_all(pool)
    .await?;

    Ok(systems)
}

/// Fetch justification history for a CVE.
pub async fn fetch_cve_justifications(
    pool: &PgPool,
    cve_id: &str,
) -> Result<Vec<CveJustification>> {
    let justifications = sqlx::query_as::<_, CveJustification>(
        r#"
        SELECT 
            scj.system_id,
            scj.cve_id,
            scj.category,
            scj.reason,
            scj.updated_by,
            scj.updated_at,
            scj.created_at,
            u.username as updated_by_username
        FROM system_cve_justifications scj
        LEFT JOIN users u ON scj.updated_by = u.id
        WHERE scj.cve_id = $1
        ORDER BY scj.updated_at DESC
        "#,
    )
    .bind(cve_id)
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
    let stats = sqlx::query_as::<_, CveFleetStats>(
        r#"
        SELECT 
            total_cves,
            critical,
            high,
            medium,
            low,
            exploited,
            fixable,
            environments_affected,
            total_system_cve_instances as systems_affected,
            outstanding,
            accepted,
            scheduled
        FROM view_cve_fleet_stats
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok(stats)
}

/// Fetch distinct package names for autocomplete.
pub async fn fetch_package_names(pool: &PgPool) -> Result<Vec<String>> {
    let packages = sqlx::query_scalar::<_, String>(
        r#"
        SELECT DISTINCT package_name
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
