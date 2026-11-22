-- Bounded bundle draft revision increments for membership writes (TASK-534.8).
--
-- Migration 0345 added `trigger_bump_bundle_membership_revision`, a row-level
-- AFTER trigger on `compliance_bundle_version_policies` and on
-- `compliance_bundle_version_requirements`. It updated the parent bundle
-- version once per membership row. An exact-membership write deletes every
-- version row of the draft and inserts n rows, and the lineage trigger
-- `sync_bundle_version_membership` inserts or deletes one version row per
-- lineage row in a separate statement. One draft update therefore changed the
-- parent row up to about 3n times and increased `draft_revision` as often.
--
-- This migration replaces the row-level triggers:
--
-- * Statement-level AFTER INSERT, UPDATE and DELETE triggers with transition
--   tables collect the distinct parent `bundle_version_id` values of one
--   statement. PostgreSQL does not allow transition tables on a trigger with
--   more than one event, so each event has its own trigger. All three call
--   `bump_bundle_membership_revision()`.
-- * `bump_bundle_versions_once_per_transaction(uuid[])` increments the
--   counter of each affected mutable version (`incomplete`, `draft`,
--   `interim`) at most once per transaction. The transaction-local setting
--   `crystal_forge.bundle_revision_bumped` records the version IDs that the
--   function already handled in the current transaction. Membership writes
--   of one transaction therefore add exactly one to the counter of each
--   changed mutable version, independent of the row count and of the
--   statement count (the per-lineage statements of the lineage trigger
--   included).
--
-- Unchanged from 0345:
--
-- * `trigger_bump_bundle_draft_revision` (BEFORE UPDATE on
--   `compliance_bundle_versions`) and its function stay as they are. It
--   accepts the explicit `draft_revision + 1` UPDATE of the membership
--   function and still increments the counter once per UPDATE that changes a
--   business column of a mutable version. Version-row changes are not
--   deduplicated.
-- * Only a mutable version changes. The immutability triggers reject
--   membership changes of an `accepted` or `deprecated` version before the
--   AFTER triggers run.
-- * Browser lineage edits, draft derivation and MCP exact-membership writes
--   all still increase the counter: each of them writes membership rows in a
--   transaction, and the first such statement of the transaction increments
--   the counter of each affected mutable version.
-- * A statement that changes no row has empty transition tables and writes
--   nothing. A no-op update writes no membership row, so the counter stays.
--
-- INVARIANT: The optimistic-concurrency check needs only that every
-- committed transaction that changes the membership of a mutable version
-- increases its counter. Readers outside the transaction see only the
-- committed counter, so one increment per transaction is sufficient.
--
-- A rolled-back transaction or savepoint reverts the increment and the
-- transaction-local setting together, so a later statement in the same
-- transaction increments the counter again.
--
-- SECURITY: `crystal_forge.bundle_revision_bumped` is not an authorization
-- boundary. A session that sets it can suppress an increment only in its own
-- transaction, and such a session can also update the counter directly.
-- Application code never sets it.

DROP TRIGGER IF EXISTS trigger_bump_bundle_membership_revision
    ON compliance_bundle_version_policies;
DROP TRIGGER IF EXISTS trigger_bump_bundle_membership_revision
    ON compliance_bundle_version_requirements;
DROP FUNCTION IF EXISTS bump_bundle_membership_revision();

CREATE OR REPLACE FUNCTION bump_bundle_versions_once_per_transaction(version_ids uuid[])
RETURNS void LANGUAGE plpgsql AS $$
DECLARE
    bumped uuid[] := COALESCE(
        NULLIF(current_setting('crystal_forge.bundle_revision_bumped', true), '')::uuid[],
        '{}'::uuid[]
    );
    pending uuid[];
BEGIN
    SELECT COALESCE(array_agg(DISTINCT id), '{}'::uuid[])
    INTO pending
    FROM unnest(version_ids) AS id
    WHERE id IS NOT NULL
      AND NOT (id = ANY (bumped));

    IF cardinality(pending) = 0 THEN
        RETURN;
    END IF;

    UPDATE compliance_bundle_versions
    SET draft_revision = draft_revision + 1
    WHERE id = ANY (pending)
      AND publication_state IN ('incomplete', 'draft', 'interim');

    -- An immutable version is recorded too: it cannot become mutable again,
    -- so a later increment in this transaction would also be a no-op.
    PERFORM set_config(
        'crystal_forge.bundle_revision_bumped',
        (bumped || pending)::text,
        true
    );
END;
$$;

CREATE OR REPLACE FUNCTION bump_bundle_membership_revision()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    -- Each trigger declares only the transition tables of its event, so each
    -- branch reads only those tables.
    IF TG_OP = 'INSERT' THEN
        PERFORM bump_bundle_versions_once_per_transaction(
            ARRAY(SELECT bundle_version_id FROM new_rows)
        );
    ELSIF TG_OP = 'UPDATE' THEN
        PERFORM bump_bundle_versions_once_per_transaction(
            ARRAY(
                SELECT bundle_version_id FROM old_rows
                UNION ALL
                SELECT bundle_version_id FROM new_rows
            )
        );
    ELSIF TG_OP = 'DELETE' THEN
        PERFORM bump_bundle_versions_once_per_transaction(
            ARRAY(SELECT bundle_version_id FROM old_rows)
        );
    END IF;
    RETURN NULL;
END;
$$;

CREATE TRIGGER trigger_bump_bundle_membership_revision_insert
    AFTER INSERT ON compliance_bundle_version_policies
    REFERENCING NEW TABLE AS new_rows
    FOR EACH STATEMENT
    EXECUTE FUNCTION bump_bundle_membership_revision();

CREATE TRIGGER trigger_bump_bundle_membership_revision_update
    AFTER UPDATE ON compliance_bundle_version_policies
    REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows
    FOR EACH STATEMENT
    EXECUTE FUNCTION bump_bundle_membership_revision();

CREATE TRIGGER trigger_bump_bundle_membership_revision_delete
    AFTER DELETE ON compliance_bundle_version_policies
    REFERENCING OLD TABLE AS old_rows
    FOR EACH STATEMENT
    EXECUTE FUNCTION bump_bundle_membership_revision();

CREATE TRIGGER trigger_bump_bundle_membership_revision_insert
    AFTER INSERT ON compliance_bundle_version_requirements
    REFERENCING NEW TABLE AS new_rows
    FOR EACH STATEMENT
    EXECUTE FUNCTION bump_bundle_membership_revision();

CREATE TRIGGER trigger_bump_bundle_membership_revision_update
    AFTER UPDATE ON compliance_bundle_version_requirements
    REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows
    FOR EACH STATEMENT
    EXECUTE FUNCTION bump_bundle_membership_revision();

CREATE TRIGGER trigger_bump_bundle_membership_revision_delete
    AFTER DELETE ON compliance_bundle_version_requirements
    REFERENCING OLD TABLE AS old_rows
    FOR EACH STATEMENT
    EXECUTE FUNCTION bump_bundle_membership_revision();
