-- Limits hardening fleet aggregates to managed machines (TASK-508 AC #12).
--
-- view_hardening_fleet_summary and view_hardening_top_vulnerable_services
-- count "systems scanned" and "affected systems". Since 0311 a registration can
-- be external or reference. A reference configuration is not a machine, and an
-- external registration is configuration evidence without deployment
-- authority, so neither belongs in a fleet machine count. Their exact
-- configuration assessments remain available per derivation through
-- view_system_hardening_posture and the configuration-assurance surfaces.
--
-- Only the system filter changes. Output columns and types are unchanged.

CREATE OR REPLACE VIEW view_hardening_fleet_summary AS
WITH latest_system_scans AS (
    SELECT DISTINCT ON (s.id)
        s.id AS system_id,
        hs.id AS scan_id,
        hs.completed_at
    FROM systems s
    JOIN commits c ON c.flake_id = s.flake_id
    JOIN derivations d ON d.commit_id = c.id
        AND d.derivation_type = 'nixos'
        AND COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname) = d.derivation_name
    JOIN hardening_scans hs ON hs.derivation_id = d.id
    WHERE s.is_active = TRUE
      AND s.management_mode = 'managed'
      AND hs.status = 'completed'
    ORDER BY s.id, hs.completed_at DESC, hs.id DESC
)
SELECT
    COUNT(lss.system_id) AS total_systems_scanned,
    AVG(hs.overall_score) AS avg_fleet_score,
    SUM(hs.well_hardened_count) AS total_well_hardened_services,
    SUM(hs.moderately_hardened_count) AS total_moderately_hardened_services,
    SUM(hs.poorly_hardened_count) AS total_poorly_hardened_services,
    SUM(hs.vulnerable_count) AS total_vulnerable_services,
    SUM(hs.total_services) AS total_services_scanned,
    MAX(hs.completed_at) AS last_scan_completed
FROM latest_system_scans lss
JOIN hardening_scans hs ON hs.id = lss.scan_id;

CREATE OR REPLACE VIEW view_hardening_top_vulnerable_services AS
WITH latest_system_scans AS (
    SELECT DISTINCT ON (s.id)
        s.id AS system_id,
        hs.id AS scan_id,
        hs.completed_at
    FROM systems s
    JOIN commits c ON c.flake_id = s.flake_id
    JOIN derivations d ON d.commit_id = c.id
        AND d.derivation_type = 'nixos'
        AND COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname) = d.derivation_name
    JOIN hardening_scans hs ON hs.derivation_id = d.id
    WHERE s.is_active = TRUE
      AND s.management_mode = 'managed'
      AND hs.status = 'completed'
    ORDER BY s.id, hs.completed_at DESC, hs.id DESC
)
SELECT
    shr.service_name,
    COUNT(DISTINCT lss.system_id) AS affected_systems_count,
    AVG(shr.hardening_score) AS avg_score,
    MIN(shr.hardening_score) AS min_score,
    MAX(shr.hardening_score) AS max_score
FROM latest_system_scans lss
JOIN service_hardening_results shr ON shr.scan_id = lss.scan_id
WHERE shr.risk_level IN ('vulnerable', 'poorly_hardened')
GROUP BY shr.service_name
ORDER BY affected_systems_count DESC, avg_score ASC
LIMIT 20;
