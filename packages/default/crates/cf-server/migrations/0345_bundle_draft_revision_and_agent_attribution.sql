-- Bundle draft revision and version-level agent attribution (TASK-534.8).
--
-- `draft_revision` is an optimistic-concurrency counter for mutable bundle
-- versions. MCP bundle draft updates must send the current value; a stale
-- value fails without a write. Every existing row starts at 1.
--
-- The counter increases on every real change of a mutable version
-- (`incomplete`, `draft`, `interim`):
--
-- * `trigger_bump_bundle_draft_revision` (BEFORE UPDATE on
--   `compliance_bundle_versions`) increments the counter when an UPDATE
--   changes any column other than the digest bookkeeping columns, the
--   counter itself and the attribution columns below. This covers browser
--   lineage edits (through `sync_bundle_draft_version`), digest writes that
--   also write metadata, trust changes and MCP updates.
-- * Membership rows live in child tables. `trigger_bump_bundle_membership_revision`
--   (AFTER INSERT, UPDATE or DELETE on `compliance_bundle_version_policies`
--   and on `compliance_bundle_version_requirements`) increments the counter
--   of the parent version with an explicit `draft_revision + 1` UPDATE. The
--   BEFORE trigger accepts that explicit increment. This covers the
--   lineage membership trigger `sync_bundle_version_membership`, draft
--   derivation and MCP exact-membership writes. A trigger is used instead of
--   an explicit increment in each service, so that no writer of membership
--   rows can change a draft without changing its revision.
--
-- One statement can therefore increment the counter more than once (one per
-- membership row and one per version UPDATE); the counter only has to
-- increase. An UPDATE that changes no business column and does not change
-- the counter, including the digest round trip `pending` -> computed, keeps
-- the counter. The counter of an `accepted` or `deprecated` version never
-- changes: the BEFORE trigger keeps the old value, and the immutability
-- triggers reject every other change of such a version and of its
-- membership.
--
-- BEFORE triggers fire in name order. `trigger_bump_bundle_draft_revision`
-- sorts before `trigger_bundle_version_immutable` and
-- `trigger_bundle_version_poam_finding_lock`; neither of them changes NEW.
--
-- `created_via_agent`, `created_access_token_id` and
-- `created_access_token_name` record that an owner access token created the
-- version through MCP. `created_by` stays the token owner. The name is a
-- snapshot taken at creation, so the Compliance view can show "via agent
-- token <name>" after the token row is gone; `created_via_agent` does not
-- depend on the foreign key. Browser-created and existing versions keep the
-- defaults.
--
-- Token deletion: ON DELETE SET NULL is an UPDATE of the referencing version.
-- For an `accepted` or `deprecated` version, `enforce_bundle_version_immutability`
-- rejects that UPDATE, so deleting a token that created a published version
-- fails and the token row stays. No current code path deletes token rows.
--
-- Constant defaults make every ADD COLUMN a catalog-only change. No row is
-- rewritten and no row trigger fires.

ALTER TABLE compliance_bundle_versions
    ADD COLUMN IF NOT EXISTS draft_revision bigint NOT NULL DEFAULT 1,
    ADD COLUMN IF NOT EXISTS created_via_agent boolean NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS created_access_token_id uuid
        REFERENCES api_access_tokens (id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS created_access_token_name text;

CREATE OR REPLACE FUNCTION bump_bundle_draft_revision()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    ignored text[] := ARRAY[
        'draft_revision',
        'semantic_digest',
        'requirement_digest',
        'digest_algorithm',
        'canonicalization_version',
        'created_via_agent',
        'created_access_token_id',
        'created_access_token_name'
    ];
BEGIN
    IF OLD.publication_state IN ('incomplete', 'draft', 'interim')
       AND (
           (to_jsonb(NEW) - ignored) IS DISTINCT FROM (to_jsonb(OLD) - ignored)
           OR NEW.draft_revision IS DISTINCT FROM OLD.draft_revision
       )
    THEN
        -- INVARIANT: one UPDATE increments the counter by exactly one, also
        -- when the writer set another value.
        NEW.draft_revision := OLD.draft_revision + 1;
    ELSE
        NEW.draft_revision := OLD.draft_revision;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trigger_bump_bundle_draft_revision
    BEFORE UPDATE ON compliance_bundle_versions
    FOR EACH ROW
    EXECUTE FUNCTION bump_bundle_draft_revision();

CREATE OR REPLACE FUNCTION bump_bundle_membership_revision()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    -- The immutability guards reject membership changes of an `accepted` or
    -- `deprecated` version before this AFTER trigger runs; the state filter
    -- keeps the UPDATE a no-op in every other case.
    UPDATE compliance_bundle_versions
    SET draft_revision = draft_revision + 1
    WHERE id IN (NEW.bundle_version_id, OLD.bundle_version_id)
      AND publication_state IN ('incomplete', 'draft', 'interim');
    RETURN NULL;
END;
$$;

CREATE TRIGGER trigger_bump_bundle_membership_revision
    AFTER INSERT OR UPDATE OR DELETE ON compliance_bundle_version_policies
    FOR EACH ROW
    EXECUTE FUNCTION bump_bundle_membership_revision();

CREATE TRIGGER trigger_bump_bundle_membership_revision
    AFTER INSERT OR UPDATE OR DELETE ON compliance_bundle_version_requirements
    FOR EACH ROW
    EXECUTE FUNCTION bump_bundle_membership_revision();
