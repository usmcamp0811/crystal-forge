-- Migration: Clean up duplicate derivations before adding unique constraints
-- Step 1: Make commit_id nullable (if not already done)
ALTER TABLE derivations
    ALTER COLUMN commit_id DROP NOT NULL;

-- Step 2: Set commit_id to NULL for all package derivations (if not already done)
UPDATE
    derivations
SET
    commit_id = NULL
WHERE
    derivation_type = 'package';

-- Step 3: Clean up duplicates for commit-specific derivations
-- Keep the most recent one (highest id) and delete older duplicates
WITH duplicates AS (
    SELECT
        id,
        ROW_NUMBER() OVER (PARTITION BY commit_id,
            derivation_name,
            derivation_type ORDER BY id DESC) AS rn
    FROM
        derivations
    WHERE
        commit_id IS NOT NULL)
DELETE FROM derivations
WHERE id IN (
        SELECT
            id
        FROM
            duplicates
        WHERE
            rn > 1);

-- Step 4: Clean up duplicates for commit-independent derivations
-- Keep the most recent one (highest id) and delete older duplicates
WITH duplicates AS (
    SELECT
        id,
        ROW_NUMBER() OVER (PARTITION BY derivation_name,
            derivation_type ORDER BY id DESC) AS rn
    FROM
        derivations
    WHERE
        commit_id IS NULL)
DELETE FROM derivations
WHERE id IN (
        SELECT
            id
        FROM
            duplicates
        WHERE
            rn > 1);

-- Step 5: Now create the unique constraints
-- For commit-specific derivations (NixOS systems)
CREATE UNIQUE INDEX derivations_commit_name_type_unique ON derivations (commit_id, derivation_name, derivation_type)
WHERE
    commit_id IS NOT NULL;

-- For commit-independent derivations (packages)
CREATE UNIQUE INDEX derivations_name_type_unique_null_commit ON derivations (derivation_name, derivation_type)
WHERE
    commit_id IS NULL;

