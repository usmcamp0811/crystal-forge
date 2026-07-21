-- Supporting indexes for the set-based Flakes registry and selected-history
-- queries introduced after migration 0178 was deployed.
--
-- This is intentionally a new migration: 0178 must remain immutable after its
-- first deployment to a live database.

-- Selected commit enrichment begins with commit_id and filters to NixOS
-- derivations before ordering/deduplicating configuration names.
CREATE INDEX IF NOT EXISTS idx_derivations_commit_nixos_name_id
    ON public.derivations (commit_id, derivation_name, id DESC)
    WHERE derivation_type = 'nixos';

-- Resolve the latest state for every relevant hostname with DISTINCT ON.
CREATE INDEX IF NOT EXISTS idx_system_states_hostname_timestamp_id
    ON public.system_states (hostname, timestamp DESC, id DESC);

-- Match active systems to their effective flake configuration without
-- repeatedly scanning the systems table for each derivation.
CREATE INDEX IF NOT EXISTS idx_systems_active_flake_effective_config
    ON public.systems (
        flake_id,
        (COALESCE(NULLIF(BTRIM(system_configuration_name), ''), hostname)),
        hostname
    )
    WHERE is_active = TRUE;
