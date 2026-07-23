-- Track when each CVE last became fleet-relevant.
-- The reconciliation sweep sets this when transitioning from "not relevant" to
-- "relevant", and clears it on the reverse transition.  Previously the code
-- derived the start of the current episode from MIN(cve_scans.completed_at),
-- which could backdate a genuine recurrence to an old scan (round 16 review).

ALTER TABLE cves ADD COLUMN fleet_relevant_since timestamptz;
