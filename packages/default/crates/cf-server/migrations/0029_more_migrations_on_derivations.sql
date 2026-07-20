-- Drop the existing index if it exists (in case it was created with a different definition)
DROP INDEX IF EXISTS derivations_commit_name_type_unique;

-- Create the unique constraint that handles both NULL and non-NULL commit_ids
CREATE UNIQUE INDEX derivations_commit_name_type_unique ON derivations (COALESCE(commit_id, -1), derivation_name, derivation_type);

