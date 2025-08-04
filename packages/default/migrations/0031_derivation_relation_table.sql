-- Step 1: Create the junction table
CREATE TABLE derivation_dependencies (
    derivation_id integer NOT NULL,
    depends_on_id integer NOT NULL,
    PRIMARY KEY (derivation_id, depends_on_id)
);

-- Step 2: Migrate existing data
INSERT INTO derivation_dependencies (derivation_id, depends_on_id)
SELECT
    id,
    parent_derivation_id
FROM
    derivations
WHERE
    parent_derivation_id IS NOT NULL;

-- Step 3: Add foreign key constraints
ALTER TABLE derivation_dependencies
    ADD CONSTRAINT derivation_dependencies_derivation_id_fkey FOREIGN KEY (derivation_id) REFERENCES derivations (id) ON DELETE CASCADE;

ALTER TABLE derivation_dependencies
    ADD CONSTRAINT derivation_dependencies_depends_on_id_fkey FOREIGN KEY (depends_on_id) REFERENCES derivations (id) ON DELETE CASCADE;

-- Step 4: Add indexes
CREATE INDEX idx_derivation_dependencies_derivation_id ON derivation_dependencies (derivation_id);

CREATE INDEX idx_derivation_dependencies_depends_on_id ON derivation_dependencies (depends_on_id);

-- Step 5: Handle dependencies before dropping column
-- Drop the view that references parent_derivation_id
DROP VIEW IF EXISTS derivations_with_status;

-- Drop constraints and indexes
DROP INDEX IF EXISTS idx_derivations_parent_id;

ALTER TABLE derivations
    DROP CONSTRAINT IF EXISTS derivations_parent_derivation_id_fkey;

-- Now drop the column
ALTER TABLE derivations
    DROP COLUMN parent_derivation_id;

-- Recreate the view without parent_derivation_id
CREATE VIEW derivations_with_status AS
SELECT
    d.id,
    d.commit_id,
    d.derivation_type,
    d.derivation_name,
    d.derivation_path,
    d.scheduled_at,
    d.completed_at,
    d.attempt_count,
    d.started_at,
    d.evaluation_duration_ms,
    d.error_message,
    d.pname,
    d.version,
    d.status_id,
    ds.name AS status_name,
    ds.description AS status_description,
    ds.is_terminal,
    ds.is_success,
    ds.display_order
FROM
    derivations d
    JOIN derivation_statuses ds ON (d.status_id = ds.id);

