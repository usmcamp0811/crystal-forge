-- Add per-event restart classification to system_states.
--
-- PROBLEM:
-- systems.last_restart_type only stores the MOST RECENT classification.
-- The system history view shows every startup entry as "System restarted"
-- because the handler maps change_reason="startup" → event_kind="restart"
-- without knowing whether it was a system reboot or an agent restart.
--
-- FIX:
-- Add restart_type to system_states so each startup row carries its own
-- authoritative classification. The heartbeat handler already computes
-- this via classify_restart_type(); this migration gives it a home.

ALTER TABLE public.system_states
    ADD COLUMN IF NOT EXISTS restart_type text
        CHECK (restart_type IS NULL
               OR restart_type IN ('system_reboot', 'agent_restart', 'unknown'));

COMMENT ON COLUMN public.system_states.restart_type IS
    'Authoritative restart classification for this state transition. '
    'Values: system_reboot (boot_id changed), agent_restart (boot_id unchanged + startup), '
    'unknown (older agent with no boot_id, or first upgraded heartbeat). '
    'NULL for non-startup transitions (heartbeats, config changes, deploys).';
