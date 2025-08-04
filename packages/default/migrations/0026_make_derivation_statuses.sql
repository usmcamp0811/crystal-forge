-- Simpler approach: Handle the status column migration step by step
-- Step 1: Create the status table first (this should work)
CREATE TABLE IF NOT EXISTS derivation_statuses (
    id integer PRIMARY KEY,
    name varchar(50) NOT NULL UNIQUE,
    description text,
    is_terminal boolean DEFAULT FALSE NOT NULL,
    is_success boolean DEFAULT FALSE NOT NULL,
    display_order integer NOT NULL UNIQUE
);

-- Step 2: Insert status values
INSERT INTO derivation_statuses (id, name, description, is_terminal, is_success, display_order)
    VALUES (1, 'pending', 'Waiting to be processed', FALSE, FALSE, 10),
    (2, 'queued', 'Queued for processing', FALSE, FALSE, 20),
    (3, 'dry-run-pending', 'Waiting for dry run evaluation', FALSE, FALSE, 30),
    (4, 'dry-run-inprogress', 'Dry run evaluation in progress', FALSE, FALSE, 40),
    (5, 'dry-run-complete', 'Dry run evaluation completed', FALSE, TRUE, 50),
    (6, 'dry-run-failed', 'Dry run evaluation failed', TRUE, FALSE, 51),
    (7, 'build-pending', 'Waiting for build', FALSE, FALSE, 60),
    (8, 'build-inprogress', 'Build in progress', FALSE, FALSE, 70),
    (9, 'in-progress', 'Processing in progress', FALSE, FALSE, 75),
    (10, 'build-complete', 'Build completed successfully', FALSE, TRUE, 80),
    (11, 'complete', 'Processing completed successfully', TRUE, TRUE, 90),
    (12, 'build-failed', 'Build failed', TRUE, FALSE, 81),
    (13, 'failed', 'Processing failed', TRUE, FALSE, 91)
ON CONFLICT (id)
    DO NOTHING;

-- Step 3: Add status_id column
ALTER TABLE derivations
    ADD COLUMN IF NOT EXISTS status_id integer REFERENCES derivation_statuses (id);

-- Step 4: Populate status_id
UPDATE
    derivations
SET
    status_id = (
        SELECT
            id
        FROM
            derivation_statuses
        WHERE
            name = derivations.status)
WHERE
    status_id IS NULL;

-- Step 5: Set NOT NULL constraint
ALTER TABLE derivations
    ALTER COLUMN status_id SET NOT NULL;

-- Step 6: Add index
CREATE INDEX IF NOT EXISTS idx_derivations_status_id ON derivations (status_id);

-- Step 7: Create a view that includes both old and new status for transition period
CREATE OR REPLACE VIEW derivations_with_both_status AS
SELECT
    d.*,
    ds.name AS status_name,
    ds.description AS status_description,
    ds.is_terminal,
    ds.is_success,
    ds.display_order
FROM
    derivations d
    JOIN derivation_statuses ds ON d.status_id = ds.id;

-- NOTE: We'll drop the status column in a separate migration after updating all dependent objects
