ALTER TABLE flakes
    ADD COLUMN IF NOT EXISTS sync_status text NOT NULL DEFAULT 'unknown',
    ADD COLUMN IF NOT EXISTS last_sync_at timestamptz,
    ADD COLUMN IF NOT EXISTS last_sync_error text;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'flakes_sync_status_check'
    ) THEN
        ALTER TABLE flakes
            ADD CONSTRAINT flakes_sync_status_check
            CHECK (sync_status IN ('unknown', 'synced', 'syncing', 'error'));
    END IF;
END $$;
