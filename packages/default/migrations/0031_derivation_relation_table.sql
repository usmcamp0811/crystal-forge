-- Drop old single-parent relationship
ALTER TABLE derivations
    DROP COLUMN IF EXISTS parent_derivation_id;

-- Create new many-to-many relationship table
CREATE TABLE derivation_dependencies (
    derivation_id uuid NOT NULL REFERENCES derivations (id) ON DELETE CASCADE,
    depends_on_id uuid NOT NULL REFERENCES derivations (id) ON DELETE CASCADE,
    PRIMARY KEY (derivation_id, depends_on_id)
);

