-- Build attempts are immutable history rows. Only active execution is unique.
-- Migration 0190 introduced attempt lineage, but migration 0184's global index
-- still prevented terminal attempts from gaining manual or automatic children.

DROP INDEX idx_build_jobs_derivation_unique;

-- INVARIANT: A derivation has at most one execution that a builder can claim,
-- run, or finish cancelling. Terminal attempts remain available as lineage and
-- audit history. The separate automatic_retry_source_id index continues to
-- permit at most one automatic child for each failed source attempt.
--
-- Superseding recovery contract: authoritative same-revision evaluation MUST
-- insert a queued child with a new UUID and the next derivation attempt number.
-- It MUST NOT revive or clear the failed evaluator_contract_obsolete source.
-- Manual retry, automatic retry, and authoritative replacement serialize on the
-- same derivation advisory lock before row locks and queue-order allocation.
CREATE UNIQUE INDEX build_jobs_one_active_execution_per_derivation
    ON build_jobs (derivation_id)
    WHERE status IN ('queued', 'building', 'cancelling');

COMMENT ON INDEX build_jobs_one_active_execution_per_derivation IS
    'Allows immutable terminal attempt history while limiting each derivation to one queued, building, or cancelling execution.';
