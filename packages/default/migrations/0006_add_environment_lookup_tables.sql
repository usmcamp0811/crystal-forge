-- Create compliance levels lookup table
CREATE TABLE IF NOT EXISTS compliance_levels (
    id serial PRIMARY KEY,
    name varchar(50) UNIQUE NOT NULL, -- e.g., PCI, HIPAA, FISMA, NONE
    description text
);

-- Create risk profiles lookup table
CREATE TABLE IF NOT EXISTS risk_profiles (
    id serial PRIMARY KEY,
    name varchar(50) UNIQUE NOT NULL,
    description text
);

-- Create shared update trigger function
CREATE OR REPLACE FUNCTION update_updated_at_column ()
    RETURNS TRIGGER
    AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$
LANGUAGE 'plpgsql';

-- Insert default compliance levels
INSERT INTO compliance_levels (name, description)
    VALUES ('NONE', 'No specific compliance requirements'),
    ('PCI', 'Payment Card Industry compliance'),
    ('HIPAA', 'Health Insurance Portability and Accountability Act'),
    ('FISMA', 'Federal Information Security Management Act'),
    -- DoD Security Technical Implementation Guides
    ('STIG', 'DoD Security Technical Implementation Guide'),
    -- Risk Management Framework
    ('RMF', 'DoD Risk Management Framework'),
    -- DoD Information Assurance Certification and Accreditation Process
    ('DIACAP', 'DoD Information Assurance Certification and Accreditation Process'),
    -- Federal Risk and Authorization Management Program
    ('FEDRAMP_LOW', 'FedRAMP Low Impact Level'),
    ('FEDRAMP_MODERATE', 'FedRAMP Moderate Impact Level'),
    ('FEDRAMP_HIGH', 'FedRAMP High Impact Level'),
    -- DoD Cloud Computing Security Requirements Guide
    ('CCSRG', 'DoD Cloud Computing Security Requirements Guide'),
    -- DoD Cybersecurity Maturity Model Certification
    ('CMMC_1', 'CMMC Level 1 - Basic Cyber Hygiene'),
    ('CMMC_2', 'CMMC Level 2 - Intermediate Cyber Hygiene'),
    ('CMMC_3', 'CMMC Level 3 - Good Cyber Hygiene'),
    ('CMMC_4', 'CMMC Level 4 - Proactive'),
    ('CMMC_5', 'CMMC Level 5 - Advanced/Progressive'),
    -- DoD Enterprise DevSecOps Reference Design
    ('DEVSECOPS', 'DoD Enterprise DevSecOps Reference Design'),
    -- Authority to Operate levels
    ('ATO_LOW', 'Authority to Operate - Low Impact'),
    ('ATO_MODERATE', 'Authority to Operate - Moderate Impact'),
    ('ATO_HIGH', 'Authority to Operate - High Impact'),
    -- National Institute of Standards and Technology
    ('NIST_800_53_LOW', 'NIST 800-53 Low Baseline'),
    ('NIST_800_53_MODERATE', 'NIST 800-53 Moderate Baseline'),
    ('NIST_800_53_HIGH', 'NIST 800-53 High Baseline'),
    -- DoD Instruction 8510.01
    ('DI_8510_01', 'DoD Instruction 8510.01 - Risk Management Framework'),
    -- Common Access Card compliance
    ('CAC', 'Common Access Card PKI compliance'),
    -- Controlled Unclassified Information
    ('CUI', 'Controlled Unclassified Information'),
    -- For classified systems
    ('SECRET', 'Secret classification level'),
    ('TOP_SECRET', 'Top Secret classification level'),
    ('TS_SCI', 'Top Secret/Sensitive Compartmented Information')
ON CONFLICT (name)
    DO NOTHING;

-- Insert default risk profiles
INSERT INTO risk_profiles (name, description)
    VALUES ('LOW', 'Low risk systems'),
    ('MEDIUM', 'Medium risk systems'),
    ('HIGH', 'High risk systems'),
    ('CRITICAL', 'Critical risk systems')
ON CONFLICT (name)
    DO NOTHING;

---
-- Migration 002: Create user management
-- migrations/002_create_users.sql
-- Create the user_type enum
CREATE TYPE user_type AS ENUM (
    'human',
    'service',
    'system'
);

-- Create the users table
CREATE TABLE IF NOT EXISTS users (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid (),
    username varchar(50) UNIQUE NOT NULL,
    first_name varchar(100) NOT NULL,
    last_name varchar(100) NOT NULL,
    email varchar(255) UNIQUE NOT NULL,
    user_type user_type NOT NULL DEFAULT 'human',
    is_active boolean NOT NULL DEFAULT TRUE,
    created_at timestamptz NOT NULL DEFAULT NOW(),
    updated_at timestamptz NOT NULL DEFAULT NOW()
);

-- Create indexes
CREATE INDEX IF NOT EXISTS idx_users_email ON users (email);

CREATE INDEX IF NOT EXISTS idx_users_type ON users (user_type);

-- Create update trigger
CREATE TRIGGER update_users_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column ();

---
-- Migration 003: Create environments
-- migrations/003_create_environments.sql
-- Create environment table
CREATE TABLE IF NOT EXISTS environments (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid (),
    name varchar(50) NOT NULL UNIQUE,
    description text,
    is_active boolean DEFAULT TRUE,
    compliance_level_id integer REFERENCES compliance_levels (id),
    risk_profile_id integer REFERENCES risk_profiles (id),
    created_by varchar(100),
    updated_by varchar(100),
    created_at timestamptz DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamptz DEFAULT CURRENT_TIMESTAMP
);

-- Create indexes
CREATE INDEX idx_environment_name ON environments (name);

-- Create update trigger
CREATE TRIGGER trigger_environment_updated_at
    BEFORE UPDATE ON environments
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column ();

-- Insert default environments
INSERT INTO environments (name, description, compliance_level_id, risk_profile_id)
SELECT
    env.name,
    env.description,
    cl.id,
    rp.id
FROM (
    VALUES ('dev', 'Development environment', 'NONE', 'LOW'),
        ('test', 'Testing environment', 'NONE', 'LOW'),
        ('preprod', 'Pre-production environment', 'NONE', 'MEDIUM'),
        ('prod', 'Production environment', 'NONE', 'HIGH')) AS env (name, description, compliance, risk)
    JOIN compliance_levels cl ON cl.name = env.compliance
    JOIN risk_profiles rp ON rp.name = env.risk
ON CONFLICT (name)
    DO NOTHING;

