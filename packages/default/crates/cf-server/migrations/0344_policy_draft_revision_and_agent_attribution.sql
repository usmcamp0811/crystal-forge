-- Policy draft revision and version-level agent attribution (TASK-534.7).
--
-- `draft_revision` is an optimistic-concurrency counter for mutable policy
-- versions. MCP draft updates must send the current value; a stale value
-- fails without a write. Every existing row starts at 1.
--
-- `trigger_bump_policy_draft_revision` increments the counter in the same
-- UPDATE when a mutable version (`incomplete`, `draft`, `interim`) changes
-- any column other than the digest bookkeeping columns, the counter itself
-- and the attribution columns below. All writers are covered without code
-- changes: browser lineage edits (through `sync_policy_draft_version`),
-- `write_policy_version_digest`, mapping digest refreshes after a mapping
-- change, trust changes and MCP draft updates. A browser edit can therefore
-- increment the counter more than once in one transaction; the counter only
-- has to increase. An UPDATE that changes no such column, including the
-- digest round trip `pending` -> computed of the same content, keeps the
-- counter. The counter of an `accepted` or `deprecated` version never
-- changes: the trigger keeps the old value, and
-- `enforce_policy_version_immutability` rejects every other change.
--
-- BEFORE triggers fire in name order. `trigger_bump_policy_draft_revision`
-- sorts before `trigger_deployment_policy_version_poam_finding_lock` and
-- `trigger_policy_version_immutable`; neither of them changes NEW.
--
-- `created_via_agent`, `created_access_token_id` and
-- `created_access_token_name` record that an owner access token created the
-- version through MCP. `created_by` stays the token owner. The name is a
-- snapshot taken at creation, so the Policies view can show "via agent token
-- <name>" after the token row is gone; `created_via_agent` does not depend on
-- the foreign key. Browser-created and existing versions keep the defaults.
--
-- Token deletion: ON DELETE SET NULL is an UPDATE of the referencing version.
-- For an `accepted` or `deprecated` version, `enforce_policy_version_immutability`
-- rejects that UPDATE, so deleting a token that created a published version
-- fails and the token row stays. No current code path deletes token rows.
--
-- Constant defaults make every ADD COLUMN a catalog-only change. No row is
-- rewritten and no row trigger fires.

ALTER TABLE deployment_policy_versions
    ADD COLUMN IF NOT EXISTS draft_revision bigint NOT NULL DEFAULT 1,
    ADD COLUMN IF NOT EXISTS created_via_agent boolean NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS created_access_token_id uuid
        REFERENCES api_access_tokens (id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS created_access_token_name text;

CREATE OR REPLACE FUNCTION bump_policy_draft_revision()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    ignored text[] := ARRAY[
        'draft_revision',
        'semantic_digest',
        'digest_algorithm',
        'canonicalization_version',
        'created_via_agent',
        'created_access_token_id',
        'created_access_token_name'
    ];
BEGIN
    IF OLD.publication_state IN ('incomplete', 'draft', 'interim')
       AND (to_jsonb(NEW) - ignored) IS DISTINCT FROM (to_jsonb(OLD) - ignored)
    THEN
        NEW.draft_revision := OLD.draft_revision + 1;
    ELSE
        -- INVARIANT: only this trigger changes the counter.
        NEW.draft_revision := OLD.draft_revision;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trigger_bump_policy_draft_revision
    BEFORE UPDATE ON deployment_policy_versions
    FOR EACH ROW
    EXECUTE FUNCTION bump_policy_draft_revision();
