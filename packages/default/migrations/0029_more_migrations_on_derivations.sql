-- Single unique constraint that handles both NULL and non-NULL commit_ids
CREATE UNIQUE INDEX derivations_commit_name_type_unique ON derivations (COALESCE(commit_id, -1), derivation_name, derivation_type);

