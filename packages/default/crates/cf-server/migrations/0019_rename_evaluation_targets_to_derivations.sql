-- Migration 001: Rename evaluation_targets to derivations and add package support
-- This consolidates nix_packages into a unified derivations table
-- Step 1: Rename the table
ALTER TABLE evaluation_targets RENAME TO derivations;

-- Step 2: Rename the sequence
ALTER SEQUENCE tbl_evaluation_targets_id_seq
    RENAME TO derivations_id_seq;

-- Step 3: Add new columns for package information and hierarchical relationships
ALTER TABLE derivations
    ADD COLUMN parent_derivation_id integer REFERENCES derivations (id) ON DELETE CASCADE,
    ADD COLUMN package_name character varying(255),
    ADD COLUMN package_pname character varying(255),
    ADD COLUMN package_version character varying(100);

-- Step 4: Update target_type constraint to include 'package' and rename to derivation_type
ALTER TABLE derivations RENAME COLUMN target_type TO derivation_type;

ALTER TABLE derivations RENAME COLUMN target_name TO derivation_name;

-- Step 5: Update derivation_type constraint
ALTER TABLE derivations
    DROP CONSTRAINT IF EXISTS valid_status;

ALTER TABLE derivations
    ADD CONSTRAINT valid_derivation_type CHECK (derivation_type IN ('nixos', 'package'));

-- Step 6: Re-add the status constraint
ALTER TABLE derivations
    ADD CONSTRAINT valid_status CHECK (status IN ('dry-run-pending', 'dry-run-inprogress', 'dry-run-complete', 'dry-run-failed', 'build-pending', 'build-inprogress', 'build-complete', 'build-failed', 'pending', 'queued', 'in-progress', 'complete', 'failed'));

-- Step 7: Add indexes for the new columns
CREATE INDEX idx_derivations_parent_id ON derivations (parent_derivation_id);

CREATE INDEX idx_derivations_derivation_type ON derivations (derivation_type);

CREATE INDEX idx_derivations_package_pname_version ON derivations (package_pname, package_version);

-- Step 8: Update existing indexes that referenced old column names
DROP INDEX IF EXISTS idx_evaluation_targets_status;

CREATE INDEX idx_derivations_status ON derivations (status);

