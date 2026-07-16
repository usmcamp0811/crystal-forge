-- Guard concurrent sync attempts from overwriting each other's results.
--
-- sync_flake_recorded() unconditionally wrote 'syncing' then unconditionally
-- wrote 'synced'/'error', so a slow attempt could finish after a newer one
-- and overwrite its result (e.g. A starts, B starts and succeeds, A fails and
-- leaves the flake in 'error' despite the newer successful sync).
--
-- Fix: generate a UUID when marking 'syncing' and only commit the final
-- status update if that UUID still matches the column, making the write
-- conditional on the attempt still being the most recent one.
ALTER TABLE flakes
    ADD COLUMN IF NOT EXISTS sync_attempt_id UUID;

COMMENT ON COLUMN flakes.sync_attempt_id IS
    'UUID written when sync_status is set to ''syncing''. Final status writes
     (synced/error) are conditional on this column still matching, preventing
     a stale concurrent attempt from overwriting a newer attempt''s result.';
