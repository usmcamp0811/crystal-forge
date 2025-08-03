-- Make commit_id nullable
ALTER TABLE derivations
    ALTER COLUMN commit_id DROP NOT NULL;

-- You might want to add a partial unique constraint for commit-specific derivations
CREATE UNIQUE INDEX derivations_commit_name_type_unique ON derivations (commit_id, derivation_name, derivation_type)
WHERE
    commit_id IS NOT NULL;

-- And another for commit-independent derivations
CREATE UNIQUE INDEX derivations_name_type_unique_null_commit ON derivations (derivation_name, derivation_type)
WHERE
    commit_id IS NULL;

-- Set commit_id to NULL for all package derivations
UPDATE
    derivations
SET
    commit_id = NULL
WHERE
    derivation_type = 'package';

