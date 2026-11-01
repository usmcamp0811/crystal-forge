-- TASK-508: record the commit a `pinned` managed system is held on.
--
-- The pin is declarative metadata. Saving it never sets desired_target, never
-- queues a deployment, and never changes deployment selection: automatic
-- policy already ignores `pinned` systems. Explicit deployments (including a
-- temporary deployment of another commit) keep their existing authorization.
ALTER TABLE systems
    ADD COLUMN pinned_commit_id integer REFERENCES commits(id) ON DELETE SET NULL;

COMMENT ON COLUMN systems.pinned_commit_id IS
    'Declared pin for a managed system with deployment_policy=pinned. NULL means no pin is recorded. Not a deployment request.';

-- INVARIANT: A pin exists only for a managed `pinned` registration and only
-- for a commit of its registered flake. Every writer (edit, mode conversion,
-- convert-to-manual, flake reassignment) gets this rule, so no stale pin can
-- survive a policy, mode or flake change. API validation rejects explicit
-- mismatches before this normalization applies.
CREATE FUNCTION normalize_system_pinned_commit() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.pinned_commit_id IS NOT NULL AND (
        NEW.management_mode <> 'managed'
        OR NEW.deployment_policy <> 'pinned'
        OR NOT EXISTS (SELECT 1 FROM commits c
                       WHERE c.id = NEW.pinned_commit_id
                         AND c.flake_id IS NOT DISTINCT FROM NEW.flake_id
                         AND NEW.flake_id IS NOT NULL)
    ) THEN
        NEW.pinned_commit_id := NULL;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER systems_normalize_pinned_commit
BEFORE INSERT OR UPDATE ON systems
FOR EACH ROW EXECUTE FUNCTION normalize_system_pinned_commit();
