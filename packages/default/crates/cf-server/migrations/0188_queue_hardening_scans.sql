-- Convert hardening scans from detached Tokio tasks into a durable serial queue.

-- A process that owned an in-progress scan cannot survive the server rollout.
-- Requeue those rows so the new worker can recover them.
UPDATE hardening_scans
SET status = 'pending',
    started_at = NULL,
    completed_at = NULL,
    scan_metadata = COALESCE(scan_metadata, '{}'::jsonb)
      || jsonb_build_object('requeued_by_migration', 188)
WHERE status = 'in_progress';

-- Reconcile duplicate active rows before enforcing idempotent enqueueing.
WITH ranked AS (
    SELECT id,
           row_number() OVER (
               PARTITION BY derivation_id
               ORDER BY scheduled_at ASC, created_at ASC, id ASC
           ) AS active_rank
    FROM hardening_scans
    WHERE status IN ('pending', 'in_progress')
)
UPDATE hardening_scans scans
SET status = 'failed',
    completed_at = NOW(),
    scan_metadata = COALESCE(scans.scan_metadata, '{}'::jsonb)
      || jsonb_build_object(
          'error', 'superseded duplicate active hardening scan',
          'reconciled_by_migration', 188
      )
FROM ranked
WHERE scans.id = ranked.id
  AND ranked.active_rank > 1;

CREATE UNIQUE INDEX hardening_scans_one_active_per_derivation
ON hardening_scans (derivation_id)
WHERE status IN ('pending', 'in_progress');

-- Database backstop: even multiple worker processes may run only one scan.
CREATE UNIQUE INDEX hardening_scans_one_global_in_progress
ON hardening_scans ((1))
WHERE status = 'in_progress';

CREATE INDEX hardening_scans_pending_queue
ON hardening_scans (scheduled_at ASC, id ASC)
WHERE status = 'pending';
