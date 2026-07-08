ALTER TABLE flakes
    ADD COLUMN IF NOT EXISTS sync_status text NOT NULL DEFAULT 'unknown',
    ADD COLUMN IF NOT EXISTS last_sync_at timestamptz,
    ADD COLUMN IF NOT EXISTS last_sync_error text;

ALTER TABLE flakes
    ADD CONSTRAINT IF NOT EXISTS flakes_sync_status_check
    CHECK (sync_status IN ('unknown', 'synced', 'syncing', 'error'));
