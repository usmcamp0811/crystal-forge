-- Create systems table (assumes tbl_flakes exists from earlier migrations)
CREATE TABLE IF NOT EXISTS tbl_systems (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid (),
    hostname text NOT NULL UNIQUE,
    environment_id uuid REFERENCES tbl_environments (id),
    is_active boolean DEFAULT TRUE,
    public_key text NOT NULL,
    flake_id uuid REFERENCES tbl_flakes (id),
    derivation text NOT NULL,
    created_at timestamptz DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamptz DEFAULT CURRENT_TIMESTAMP
);

-- Create indexes
CREATE INDEX idx_tbl_systems_environment_id ON tbl_systems (environment_id);

CREATE INDEX idx_tbl_systems_hostname ON tbl_systems (hostname);

-- Create update trigger
CREATE TRIGGER trigger_tbl_systems_updated_at
    BEFORE UPDATE ON tbl_systems
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column ();

-- Migrate any existing systems to dev environment
UPDATE
    tbl_systems
SET
    environment_id = (
        SELECT
            id
        FROM
            tbl_environments
        WHERE
            name = 'dev'
        LIMIT 1)
WHERE
    environment_id IS NULL;

