-- Track when each CVE last became fleet-relevant.
-- The reconciliation sweep sets this when transitioning from "not relevant" to
-- "relevant", and clears it on the reverse transition.  Previously the code
-- derived the start of the current episode from MIN(cve_scans.completed_at),
-- which could backdate a genuine recurrence to an old scan (round 16 review).

ALTER TABLE cves ADD COLUMN fleet_relevant_since timestamptz;

-- Backfill the column for CVEs that are currently fleet-relevant and already
-- have an open attention occurrence.  This ensures existing incidents keep
-- their original opened_at instead of being reset to the migration time.
UPDATE cves c
SET fleet_relevant_since = current_incident.opened_at
FROM (
    SELECT ao.subject_id, MIN(ao.opened_at) AS opened_at
    FROM attention_occurrences ao
    JOIN view_cve_list_with_metadata v
      ON v.cve_id = ao.subject_id
     AND v.severity = 'CRITICAL'
     AND v.affected_count > 0
    WHERE ao.category = 'cves'
      AND ao.resolved_at IS NULL
    GROUP BY ao.subject_id
) current_incident
WHERE c.id = current_incident.subject_id;
