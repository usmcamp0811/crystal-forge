-- Crystal Forge agent enablement is now enforced through a built-in
-- unconditional invariant rather than an assignable deployment policy.
-- Legacy require_cf_agent records must be disabled and their assignments
-- removed so they no longer appear as a separate matrix column.
--
-- The deployment_policy record itself is preserved for audit/history.
-- Assignments are removed so the evaluator no longer generates a
-- duplicate Nix result key for what is already covered by the global
-- cfAgentEnabled metadata.

-- 1. Remove environment assignments for legacy require_cf_agent policies.
WITH legacy_cf_agent AS (
    SELECT id FROM deployment_policies
    WHERE policy_type = 'require_cf_agent'
)
DELETE FROM environment_policies
WHERE policy_id IN (SELECT id FROM legacy_cf_agent);

-- 2. Remove direct system assignments for legacy require_cf_agent policies.
WITH legacy_cf_agent AS (
    SELECT id FROM deployment_policies
    WHERE policy_type = 'require_cf_agent'
)
DELETE FROM system_policy_assignments
WHERE policy_id IN (SELECT id FROM legacy_cf_agent);

-- 3. Disable the legacy policy records themselves.
UPDATE deployment_policies
SET enabled = FALSE,
    updated_at = NOW()
WHERE policy_type = 'require_cf_agent'
  AND enabled IS NOT FALSE;
