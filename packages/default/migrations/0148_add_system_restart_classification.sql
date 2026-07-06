-- Persist authoritative restart classification on systems.
--
-- The server compares the incoming boot_id against the stored value on every
-- heartbeat and startup POST. This migration adds two columns so the outcome
-- is durably recorded and surfaced through the API/UI instead of only being
-- emitted as a log message:
--
--   last_restart_type  'system_reboot' | 'agent_restart' | 'unknown'
--   last_restart_at    timestamp of the event that was classified
--
-- Classification rules (server-side, in handlers/agent/heartbeat.rs):
--   boot_id changed          → 'system_reboot'
--   boot_id unchanged and    → 'agent_restart'
--     change_reason = 'startup'
--   no boot_id in payload    → 'unknown'  (older agent; not updated)
--   boot_id Initialized      → 'agent_restart' (first upgraded heartbeat,
--                               not a reboot)
--
-- NULL means no startup event has been processed yet for this system.

ALTER TABLE public.systems
    ADD COLUMN IF NOT EXISTS last_restart_type text
        CHECK (last_restart_type IS NULL OR last_restart_type IN ('system_reboot', 'agent_restart', 'unknown')),
    ADD COLUMN IF NOT EXISTS last_restart_at timestamptz;

COMMENT ON COLUMN public.systems.last_restart_type IS
    'Authoritative restart classification written by the server on each startup heartbeat. '
    'Values: system_reboot (boot_id changed), agent_restart (boot_id unchanged or first heartbeat), '
    'unknown (older agent with no boot_id). NULL = no startup event processed yet.';

COMMENT ON COLUMN public.systems.last_restart_at IS
    'Timestamp of the heartbeat that triggered the last restart classification.';
