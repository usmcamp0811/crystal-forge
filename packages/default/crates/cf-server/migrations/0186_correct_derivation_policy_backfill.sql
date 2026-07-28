-- Correct the unsafe backfill performed by migration 0185.
--
-- 0185 set policy_requirements_met = cf_agent_enabled for every row, which
-- incorrectly infers that cf_agent_enabled = TRUE means every assigned
-- strict policy also passed (e.g. a require_packages policy could have
-- failed independently while the agent was enabled). Only an explicitly
-- disabled agent (cf_agent_enabled = FALSE) is a provable policy failure;
-- everything else must be treated as unknown until the configuration is
-- re-evaluated and a real policy_results document is populated.
--
-- This UPDATE is scoped to rows that still carry 0185's inferred value and
-- have no real policy_results document (policy_results = '{}'), so it does
-- not touch rows that have since been re-evaluated under the new model.
UPDATE derivations
SET policy_requirements_met = NULL
WHERE policy_results = '{}'::jsonb
  AND cf_agent_enabled IS TRUE
  AND policy_requirements_met IS TRUE;
