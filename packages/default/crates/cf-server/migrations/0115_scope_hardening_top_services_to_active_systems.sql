-- Scope hardening top-service counts to active systems only.
--
-- Existing view_hardening_top_vulnerable_services counts DISTINCT derivation_id
-- across latest-per-derivation scans. That can overcount service impact because a
-- single active system may have many historical derivations/scans.
--
-- Fix: for each active system, select only its latest completed hardening scan,
-- then aggregate vulnerable/poorly_hardened services from that active-system scan
-- set. This makes affected_systems_count match current active configs.

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
