-- Create systems table (assumes flakes exists from earlier migrations)
CREATE TABLE IF NOT EXISTS systems (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid (),
    hostname text NOT NULL UNIQUE,
    environment_id uuid REFERENCES environments (id),
    is_active boolean DEFAULT TRUE,
    public_key text NOT NULL,
    flake_id int REFERENCES tbl_flakes (id),
    derivation text NOT NULL,
    created_at timestamptz DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamptz DEFAULT CURRENT_TIMESTAMP
);

-- Create indexes
CREATE INDEX idx_systems_environment_id ON systems (environment_id);

CREATE INDEX idx_systems_hostname ON systems (hostname);

-- Create update trigger
CREATE TRIGGER trigger_systems_updated_at
    BEFORE UPDATE ON systems
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column ();

-- Migrate any existing systems to dev environment
UPDATE
    systems
SET
    environment_id = (
        SELECT
            id
        FROM
            environments
        WHERE
            name = 'dev'
        LIMIT 1)
WHERE
    environment_id IS NULL;

