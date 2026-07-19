CREATE OR REPLACE VIEW view_nixos_system_build_progress AS
WITH nixos_systems AS (
    SELECT
        d.id AS system_id,
        d.derivation_name AS system_name,
        d.commit_id,
        d.attempt_count,
        c.git_commit_hash,
        c.commit_timestamp,
        ds.name AS system_status,
        ds.is_success AS system_complete
    FROM
        derivations d
        JOIN derivation_statuses ds ON d.status_id = ds.id
        LEFT JOIN commits c ON d.commit_id = c.id
    WHERE
        d.derivation_type = 'nixos'
),
package_progress AS (
    SELECT
        ns.system_id,
        ns.system_name,
        ns.commit_id,
        ns.attempt_count,
        ns.git_commit_hash,
        ns.commit_timestamp,
        ns.system_status,
        ns.system_complete,
        COUNT(pkg.id) AS total_packages,
    COUNT(pkg.id) FILTER (WHERE pkg_ds.is_success = TRUE) AS completed_packages,
    COUNT(pkg.id) FILTER (WHERE pkg_ds.name IN ('build-pending', 'build-in-progress')) AS building_packages,
    COUNT(pkg.id) FILTER (WHERE pkg_ds.name LIKE '%-failed') AS failed_packages
FROM
    nixos_systems ns
    LEFT JOIN derivation_dependencies dd ON ns.system_id = dd.derivation_id
        LEFT JOIN derivations pkg ON dd.depends_on_id = pkg.id
            AND pkg.derivation_type = 'package'
        LEFT JOIN derivation_statuses pkg_ds ON pkg.status_id = pkg_ds.id
    GROUP BY
        ns.system_id,
        ns.system_name,
        ns.commit_id,
        ns.attempt_count,
        ns.git_commit_hash,
        ns.commit_timestamp,
        ns.system_status,
        ns.system_complete
)
SELECT
    system_name,
    git_commit_hash,
    commit_timestamp,
    system_status,
    total_packages,
    completed_packages,
    building_packages,
    failed_packages,
    CASE WHEN total_packages = 0 THEN
        100.0
    ELSE
        ROUND((completed_packages::numeric / total_packages::numeric) * 100.0, 1)
    END AS build_progress_percent,
    CASE WHEN system_complete = TRUE
        AND failed_packages = 0 THEN
        'READY'
    WHEN failed_packages > 0 THEN
        'FAILED'
    WHEN system_status = 'dry-run-failed'
        AND attempt_count >= 5 THEN
        'FAILED'
    WHEN building_packages > 0 THEN
        'BUILDING'
    ELSE
        'PENDING'
    END AS overall_status
FROM
    package_progress
ORDER BY
    system_name;

