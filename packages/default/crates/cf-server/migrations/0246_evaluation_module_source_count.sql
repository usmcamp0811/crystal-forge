-- TASK-440: make module_count the exact distinct provenance-tuple count.
--
-- The source tray groups by source input, source revision, and source path.
-- Backfill the scalar summary to use the same semantics for existing snapshots.
ALTER TABLE evaluation_snapshots
    ADD COLUMN snapshot_version bigint NOT NULL DEFAULT 1
        CHECK (snapshot_version > 0);

CREATE FUNCTION bump_evaluation_snapshot_version()
RETURNS trigger AS $$
BEGIN
    IF NEW.snapshot_version = OLD.snapshot_version AND NEW IS DISTINCT FROM OLD THEN
        NEW.snapshot_version := OLD.snapshot_version + 1;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER evaluation_snapshots_bump_version
BEFORE UPDATE ON evaluation_snapshots
FOR EACH ROW EXECUTE FUNCTION bump_evaluation_snapshot_version();

WITH source_counts AS (
    SELECT snapshot.id,
           (COUNT(DISTINCT ROW(
               definition.value->>'source_input',
               definition.value->>'source_revision',
               definition.value->>'source_path'
           )) FILTER (WHERE definition.value IS NOT NULL))::integer AS module_count
    FROM evaluation_snapshots snapshot
    LEFT JOIN evaluation_snapshot_options item
      ON item.snapshot_id = snapshot.id
    LEFT JOIN evaluation_option_contents content
      ON content.digest = item.content_digest
    LEFT JOIN LATERAL jsonb_array_elements(
        CASE WHEN jsonb_typeof(content.payload->'definitions') = 'array'
             THEN content.payload->'definitions' ELSE '[]'::jsonb END
    ) definition(value) ON true
    GROUP BY snapshot.id
)
UPDATE evaluation_snapshots snapshot
SET module_count = source_counts.module_count
FROM source_counts
WHERE source_counts.id = snapshot.id
  AND snapshot.module_count IS DISTINCT FROM source_counts.module_count;
