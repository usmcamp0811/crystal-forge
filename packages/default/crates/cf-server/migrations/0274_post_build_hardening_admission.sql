-- Move automatic hardening admission from commit evaluation to successful
-- exact NixOS build completion.
--
-- Migration 0188 made the hardening queue durable and serial. It did not record
-- why a scan row exists or which successful build produced it, so an automatic
-- admission path could not be made idempotent and a backfill could not tell an
-- already-covered build from an uncovered one. This migration adds that
-- provenance and the read indexes the new lifecycle and backfill queries need.
--
-- This migration intentionally enqueues nothing. Admission remains controlled by
-- `server.auto_hardening_scans`, which still defaults to false. A bulk insert
-- here would start an unbounded series of `nix eval` subprocesses on the first
-- server start after deployment.

-- Records why the scan row exists.
--
-- 'manual'     - a person or an API client asked for this scan.
-- 'post_build' - a successful exact NixOS build admitted this scan inside the
--                transaction that recorded the build success.
-- 'backfill'   - the hardening worker admitted this scan for an already
--                successfully built target that had no hardening evidence.
--
-- Existing rows predate automatic post-build admission and are therefore
-- 'manual'. The default keeps unmodified INSERT statements, such as the manual
-- API enqueue, on the manual path without further change.
ALTER TABLE hardening_scans
    ADD COLUMN source_trigger text NOT NULL DEFAULT 'manual',
    ADD COLUMN source_build_job_id uuid REFERENCES build_jobs(id) ON DELETE SET NULL;

ALTER TABLE hardening_scans
    ADD CONSTRAINT hardening_scan_source_trigger_check
    CHECK (source_trigger IN ('manual', 'post_build', 'backfill'));

-- INVARIANT: Build provenance belongs only to automatic admission. A manual
-- request is not evidence about one exact build attempt and must not consume
-- the automatic idempotency slot for that build.
ALTER TABLE hardening_scans
    ADD CONSTRAINT hardening_scan_manual_has_no_build_provenance
    CHECK (source_trigger <> 'manual' OR source_build_job_id IS NULL);

-- IDEMPOTENCY: At most one automatic hardening event may ever exist for one
-- successful build job, including after that event reaches a terminal state.
-- Repeated completion callbacks, concurrent completion of the same job, and a
-- later backfill pass therefore cannot create a second scan for the same build.
--
-- The column stays nullable because the legacy in-server build worker completes
-- a build without a `build_jobs` row. Those automatic events carry no build
-- identity and are deduplicated only by the active-scan index from 0188.
CREATE UNIQUE INDEX hardening_scans_one_automatic_event_per_build
    ON hardening_scans (source_build_job_id)
    WHERE source_build_job_id IS NOT NULL;

-- Serves the exact-target inventory source lookup, which reads the newest
-- completed scan for one derivation. Without this index that query degrades to a
-- scan of every historical attempt for the derivation.
CREATE INDEX hardening_scans_latest_completed_per_derivation
    ON hardening_scans (derivation_id, completed_at DESC, id DESC)
    WHERE status = 'completed' AND completed_at IS NOT NULL;

-- Serves the exact-target lifecycle lookup, which reads the newest attempt of
-- any status for one derivation, and the backfill candidate filter, which must
-- reject derivations that already have an active or completed attempt.
CREATE INDEX hardening_scans_latest_attempt_per_derivation
    ON hardening_scans (derivation_id, created_at DESC, id DESC);
