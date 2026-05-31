-- Scope hardening fleet summary to active systems only.
--
-- Existing view_hardening_fleet_summary aggregates latest completed scan per
-- derivation, which can overcount and misweight fleet posture when a system has
-- historical derivations or when inactive systems still have completed scans.
--
-- Fix: select latest completed hardening scan per active system, then aggregate
-- summary metrics over that active-system scan set.

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
