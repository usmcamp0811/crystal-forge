-- Registry activity is independent of observed health. Migration 0318 makes
-- Current authority active-only, but reusing that projection in detail erased
-- inactive managed telemetry and classified deactivation itself as an outage.
-- Keep the managed enrollment boundary in detail without requiring activity.
-- Current CVE/POA&M authority and agent write guards remain active-only.
DO $$
DECLARE
    definition text;
BEGIN
    definition := pg_get_viewdef('public.view_system_detail'::regclass, true);
    definition := regexp_replace(definition,
        '\m(public\.)?view_agent_current_system_states\M',
        '(SELECT observation.* FROM public.system_states observation
          JOIN public.systems enrollment ON enrollment.hostname = observation.hostname
          WHERE enrollment.management_mode = ''managed''
            AND (enrollment.agent_enrolled_at IS NULL
                 OR observation.timestamp >= enrollment.agent_enrolled_at))',
        'g');
    EXECUTE 'CREATE OR REPLACE VIEW public.view_system_detail AS ' || definition;
END;
$$;
