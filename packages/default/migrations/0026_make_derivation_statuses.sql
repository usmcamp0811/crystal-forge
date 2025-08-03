-- Migration: Create derivation_statuses table and refactor status field
-- This creates a proper status enum table with ordering for easier querying
-- Step 1: Create the derivation_statuses table with ordering
CREATE TABLE derivation_statuses (
    id integer PRIMARY KEY,
    name varchar(50) NOT NULL UNIQUE,
    description text,
    is_terminal boolean DEFAULT FALSE NOT NULL,
    is_success boolean DEFAULT FALSE NOT NULL,
    display_order integer NOT NULL UNIQUE
);

-- Step 2: Insert the status values with logical ordering
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
    (13, 'failed', 'Processing failed', TRUE, FALSE, 91);

-- Step 3: Add the new status_id column to derivations
ALTER TABLE derivations
    ADD COLUMN status_id integer REFERENCES derivation_statuses (id);

-- Step 4: Populate status_id based on current status values
UPDATE
    derivations
SET
    status_id = (
        SELECT
            id
        FROM
            derivation_statuses
        WHERE
            name = derivations.status);

-- Step 5: Add NOT NULL constraint and index
ALTER TABLE derivations
    ALTER COLUMN status_id SET NOT NULL;

CREATE INDEX idx_derivations_status_id ON derivations (status_id);

-- Step 6: Drop the old status column and constraint
ALTER TABLE derivations
    DROP CONSTRAINT IF EXISTS valid_status;

ALTER TABLE derivations
    DROP COLUMN status;

-- Step 7: Add a helpful view for status information
CREATE VIEW derivations_with_status AS
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

