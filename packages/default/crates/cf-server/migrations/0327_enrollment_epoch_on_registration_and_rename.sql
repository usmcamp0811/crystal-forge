-- A registration that claims a hostname with earlier reports starts its own
-- observation boundary.
--
-- Migration 0318 set agent_enrolled_at only when a system re-entered managed
-- mode. Two other events attach a hostname to a registration that has no
-- relationship with the runtime reports already stored under that name:
--
--   1. INSERT of a managed system on a hostname freed by an earlier removal, and
--   2. a hostname change on a managed system onto a name that has reports.
--
-- Both left the epoch NULL or unchanged, and NULL means "all history is
-- current". The earlier registration's system_states then fed
-- view_agent_current_system_states, Current CVE authority, health and POA&M
-- verification of the new registration.
--
-- INVARIANT: system_states are immutable history. An observation is current
-- only when it was stored at or after the registration's boundary. The boundary
-- is server-owned: callers cannot set it, key rotation and metadata edits keep
-- it, and each event above moves it to the wall-clock time after the row lock is
-- held, as 0318 does for re-enrollment.
--
-- A hostname with no stored report has no history to exclude. Its registration
-- keeps the previous behavior, so a first report is current without a boundary.
--
-- COMPATIBILITY: Existing rows keep their stored value (NULL for installations
-- that predate 0318), so upgrading changes no current evidence.
CREATE OR REPLACE FUNCTION protect_agent_enrollment_epoch() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        NEW.agent_enrolled_at := CASE
            WHEN NEW.management_mode = 'managed'
             AND EXISTS (SELECT 1 FROM system_states WHERE hostname = NEW.hostname)
            THEN clock_timestamp() END;
    ELSIF OLD.management_mode <> 'managed' AND NEW.management_mode = 'managed' THEN
        -- UPDATE already owns the system-row lock. Transaction-start now() can
        -- precede an old report committed while this UPDATE waited for that lock.
        -- Wall-clock time after acquiring it must exclude that report as well.
        NEW.agent_enrolled_at := clock_timestamp();
    ELSIF NEW.management_mode <> 'managed' THEN
        NEW.agent_enrolled_at := NULL;
    ELSIF NEW.hostname IS DISTINCT FROM OLD.hostname
      AND EXISTS (SELECT 1 FROM system_states WHERE hostname = NEW.hostname) THEN
        -- Reports are keyed by hostname text. Reports under the new name belong
        -- to whatever used it before.
        NEW.agent_enrolled_at := clock_timestamp();
    ELSE
        -- Ignore caller input. Key rotation, config sync and metadata edits do
        -- not start a new enrollment or erase an existing boundary.
        NEW.agent_enrolled_at := OLD.agent_enrolled_at;
    END IF;
    RETURN NEW;
END;
$$;

COMMENT ON COLUMN systems.agent_enrolled_at IS
    'Server-owned observation boundary of the managed registration. Set on re-entering managed mode and when a registration or rename claims a hostname that already has reports. NULL for managed rows that predate migration 0318 or claimed a hostname with no reports (all history current) and for nonmanaged rows. Observations before this boundary are history only.';
