-- NULL preserves Current semantics for existing managed installations. Only
-- leaving and re-entering managed enrollment establishes a new trust boundary.
ALTER TABLE systems ADD COLUMN agent_enrolled_at timestamptz;
COMMENT ON COLUMN systems.agent_enrolled_at IS
    'Server-owned managed re-enrollment boundary. NULL retains legacy semantics. Observations before this boundary are history only.';

CREATE FUNCTION protect_agent_enrollment_epoch() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        NEW.agent_enrolled_at := NULL;
    ELSIF OLD.management_mode <> 'managed' AND NEW.management_mode = 'managed' THEN
        -- UPDATE already owns the system-row lock. Transaction-start now() can
        -- precede an old report committed while this UPDATE waited for that lock.
        -- Wall-clock time after acquiring it must exclude that report as well.
        NEW.agent_enrolled_at := clock_timestamp();
    ELSIF NEW.management_mode <> 'managed' THEN
        NEW.agent_enrolled_at := NULL;
    ELSE
        -- Ignore caller input. Key rotation, config sync and metadata edits do
        -- not start a new enrollment or erase an existing boundary.
        NEW.agent_enrolled_at := OLD.agent_enrolled_at;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER systems_agent_enrollment_epoch
BEFORE INSERT OR UPDATE ON systems
FOR EACH ROW EXECUTE FUNCTION protect_agent_enrollment_epoch();

CREATE VIEW view_agent_current_system_states AS
SELECT state.* FROM system_states state
JOIN systems system ON system.hostname = state.hostname
WHERE system.is_active AND system.management_mode = 'managed'
  AND (system.agent_enrolled_at IS NULL OR state.timestamp >= system.agent_enrolled_at);

COMMENT ON VIEW view_agent_current_system_states IS
    'Full observations eligible for current managed enrollment, not just the latest row. Heartbeats on excluded historical states cannot establish health or Current authority. Raw system_states remains immutable history.';

-- Rebind only these existing current projections. pg_get_viewdef preserves all
-- output columns/types, exact scan/output rules, snapshot lineage, deployment
-- identity and view dependencies. Historical hardware comparisons stay raw.
DO $$
DECLARE
    view_name text;
    definition text;
BEGIN
    FOREACH view_name IN ARRAY ARRAY[
        'view_current_cve_authority',
        'view_system_deployment_status',
        'view_system_list',
        'view_system_detail'
    ] LOOP
        definition := pg_get_viewdef(('public.' || view_name)::regclass, true);
        definition := regexp_replace(definition,
            '\m(public\.)?system_states\M', 'public.view_agent_current_system_states', 'g');
        -- These two aliases intentionally describe history, not runtime.
        definition := replace(definition,
            'public.view_agent_current_system_states ss_recent', 'public.system_states ss_recent');
        definition := replace(definition,
            'public.view_agent_current_system_states ss_history', 'public.system_states ss_history');
        EXECUTE format('CREATE OR REPLACE VIEW public.%I AS %s', view_name, definition);
    END LOOP;

    -- The legacy closure constraint also rechecks runtime evidence in SQL.
    -- Preserve its policy/token checks, but exclude previous enrollments.
    definition := pg_get_functiondef(
        'public.poam_legacy_observation_is_authoritative(public.poam_verification_items,boolean)'::regprocedure);
    definition := regexp_replace(definition,
        '\m(public\.)?system_states\M', 'public.view_agent_current_system_states', 'g');
    EXECUTE definition;
END;
$$;
