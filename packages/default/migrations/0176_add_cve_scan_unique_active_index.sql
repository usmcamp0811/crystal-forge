-- Prevent duplicate active CVE scans for the same derivation.
--
-- The background loop and on-demand scan requests both create scan records
-- independently.  Without this constraint, two concurrent paths can both
-- create a pending scan for the same derivation, leading to duplicate work
-- and race conditions in failure handling.
--
-- `ON CONFLICT DO NOTHING` in the insert query then makes the claim atomic:
-- whichever path inserts first wins; the second silently becomes a no-op.
WITH ranked AS (
    SELECT
        id,
        row_number() OVER (
            PARTITION BY derivation_id
            ORDER BY created_at DESC, id DESC
        ) AS rn
    FROM cve_scans
    WHERE status IN ('pending', 'in_progress')
)
UPDATE cve_scans cs
SET
    status = 'failed',
    completed_at = NOW(),
    scan_metadata = COALESCE(cs.scan_metadata, '{}'::jsonb) || jsonb_build_object(
        'error', 'Superseded while enforcing one active CVE scan per derivation'
    )
WHERE cs.id IN (
    SELECT id FROM ranked WHERE rn > 1
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_cve_scans_unique_active
ON cve_scans (derivation_id)
WHERE status IN ('pending', 'in_progress');
