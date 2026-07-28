ALTER TABLE derivations
    ADD COLUMN IF NOT EXISTS policy_requirements_met BOOLEAN,
    ADD COLUMN IF NOT EXISTS policy_results JSONB NOT NULL DEFAULT '{}'::jsonb;

-- Backfill only the case we can prove: an explicitly disabled CF agent is
-- definitely a policy failure regardless of what other policies were
-- assigned. We cannot infer that cf_agent_enabled = TRUE means all assigned
-- strict policies passed (e.g. a require_packages policy could have failed
-- independently), so those rows are intentionally left NULL ("unknown").
-- NULL/unknown rows are treated as not-yet-evaluated by every downstream
-- consumer (build-job gates require policy_requirements_met = TRUE, and the
-- policy matrix/queue counters must display them as "legacy_unknown" rather
-- than passing) until the configuration is re-evaluated and the full
-- policy_results document is populated.
UPDATE derivations
SET policy_requirements_met = FALSE
WHERE policy_requirements_met IS NULL
  AND cf_agent_enabled IS FALSE;
