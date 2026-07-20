ALTER TABLE pending_system_deployments
    ADD COLUMN IF NOT EXISTS delivered_at timestamptz,
    ADD COLUMN IF NOT EXISTS applying_at timestamptz;
