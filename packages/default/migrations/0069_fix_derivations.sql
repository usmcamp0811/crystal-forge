-- Drop the current constraint that doesn't match the ON CONFLICT clause
DROP INDEX IF EXISTS public.derivations_commit_name_type_unique;

-- Create a unique index using COALESCE to handle NULL commit_id
-- This matches the ON CONFLICT (COALESCE(commit_id, -1), derivation_name, derivation_type) in your insert
CREATE UNIQUE INDEX derivations_commit_name_type_unique ON derivations (COALESCE(commit_id, -1), derivation_name, derivation_type);

