ALTER TABLE flakes
ADD COLUMN IF NOT EXISTS build_scope text NOT NULL DEFAULT 'cf_systems_only';

ALTER TABLE flakes
DROP CONSTRAINT IF EXISTS flakes_build_scope_check;

ALTER TABLE flakes
ADD CONSTRAINT flakes_build_scope_check
CHECK (build_scope IN ('all_configs', 'cf_systems_only'));

ALTER TABLE systems
ADD COLUMN IF NOT EXISTS system_configuration_name text;

UPDATE systems
SET system_configuration_name = hostname
WHERE system_configuration_name IS NULL OR btrim(system_configuration_name) = '';

CREATE TABLE IF NOT EXISTS flake_credentials (
    id serial PRIMARY KEY,
    flake_id int NOT NULL UNIQUE REFERENCES flakes (id) ON DELETE CASCADE,
    auth_type text NOT NULL,
    username text,
    secret_encrypted text,
    ssh_username text,
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT flake_credentials_auth_type_check
        CHECK (auth_type IN ('pat', 'ssh_key', 'username_password'))
);

CREATE INDEX IF NOT EXISTS idx_flake_credentials_flake_id
ON flake_credentials (flake_id);

DROP TRIGGER IF EXISTS trigger_flake_credentials_updated_at ON flake_credentials;

CREATE TRIGGER trigger_flake_credentials_updated_at
    BEFORE UPDATE ON flake_credentials
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column ();
