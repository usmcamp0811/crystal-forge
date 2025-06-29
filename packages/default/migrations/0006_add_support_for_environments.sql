CREATE TABLE IF NOT EXISTS tbl_compliance_levels (
    id serial PRIMARY KEY,
    name varchar(50) UNIQUE NOT NULL, -- e.g., PCI, HIPAA, FISMA, NONE
    description text
);

-- Create environment table to map systems to deployment environments
CREATE TABLE IF NOT EXISTS tbl_environments (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid (),
    name varchar(50) NOT NULL UNIQUE, -- dev, test, stage, prod, etc.
    description text,
    type varchar(50), -- tier/type like 'sandbox', 'regulated', etc.
    is_active boolean DEFAULT TRUE, -- allow enabling/disabling environments
    compliance_level_id int REFERENCES tbl_compliance_levels (id),
    risk_profile varchar(50), -- e.g., 'low', 'medium', 'high'
    created_by varchar(100), -- who created the environment
    updated_by varchar(100), -- who last updated the environment
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP
);

-- Create table of systems
CREATE TABLE IF NOT EXISTS tbl_systems (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid (),
    hostname text NOT NULL UNIQUE,
    environment_id uuid REFERENCES tbl_environments (id),
    is_active boolean DEFAULT TRUE, -- allow enabling/disabling system
    public_key text NOT NULL,
    flake_id uuid REFERENCES tbl_flakes (id),
    derivation text NOT NULL, -- name of the derivation
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP
);

-- Create index on name for faster lookups
CREATE INDEX idx_tbl_environment_name ON tbl_environments (name);

-- Insert default environments
INSERT INTO tbl_environments (name, description)
    VALUES ('dev', 'Development environment'),
    ('test', 'Testing environment'),
    ('preprod', 'Pre-production environment'),
    ('prod', 'Production environment')
ON CONFLICT (name)
    DO NOTHING;

-- Create index on environment_id for faster joins
CREATE INDEX idx_tbl_systems_environment_id ON tbl_systems (environment_id);

-- Update trigger for environment table
CREATE OR REPLACE FUNCTION update_tbl_environment_updated_at ()
    RETURNS TRIGGER
    AS $
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$
LANGUAGE plpgsql;

CREATE TRIGGER trigger_tbl_environment_updated_at
    BEFORE UPDATE ON tbl_environments
    FOR EACH ROW
    EXECUTE FUNCTION update_tbl_environment_updated_at ();

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

