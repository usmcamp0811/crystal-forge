-- TASK-440: Persist optional actual local Nix closure measurements.
--
-- Older rows and servers without the complete closure in their local store keep
-- NULL. The server never substitutes derivation metadata or snapshot JSON size.
ALTER TABLE derivations
    ADD COLUMN closure_size_bytes bigint
        CHECK (closure_size_bytes IS NULL OR closure_size_bytes >= 0);

COMMENT ON COLUMN derivations.closure_size_bytes IS
    'Sum of narSize for each unique store path returned by one successful recursive Nix path-info query; NULL means unavailable.';

-- Host delta is materialized because calculating a same-commit modal corpus is
-- proportional to every configuration and option at that commit. The summary
-- endpoint must remain a scalar read.
ALTER TABLE evaluation_snapshots
    ADD COLUMN host_delta_count bigint
        CHECK (host_delta_count IS NULL OR host_delta_count >= 0);

COMMENT ON COLUMN evaluation_snapshots.host_delta_count IS
    'Option states that differ from the deterministic same-commit modal state; NULL means the snapshot is not usable. Digests include the complete safe option state, so provenance-only differences count.';

CREATE FUNCTION recompute_evaluation_host_deltas(target_commit_id integer)
RETURNS void AS $$
BEGIN
    UPDATE evaluation_snapshots
    SET host_delta_count = NULL
    WHERE commit_id = target_commit_id;

    WITH usable_snapshots AS (
        SELECT snapshot.id, snapshot.commit_id
        FROM evaluation_snapshots snapshot
        WHERE snapshot.commit_id = target_commit_id
          AND snapshot.lifecycle = 'available'
          AND snapshot.schema_version = 1
          AND snapshot.option_count = (
              SELECT COUNT(*)
              FROM evaluation_snapshot_options item
              WHERE item.snapshot_id = snapshot.id
          )
          AND NOT EXISTS (
              SELECT 1
              FROM evaluation_snapshot_options item
              LEFT JOIN evaluation_option_contents content
                ON content.digest = item.content_digest
              WHERE item.snapshot_id = snapshot.id
                AND (content.digest IS NULL OR content.schema_version <> 1
                  OR jsonb_typeof(content.payload) <> 'object'
                  OR NOT content.payload ?& ARRAY[
                      'declared_type', 'value', 'definitions', 'overridden'
                  ]
                  OR jsonb_typeof(content.payload->'declared_type') <> 'string'
                  OR jsonb_typeof(content.payload->'overridden') <> 'boolean'
                  OR jsonb_typeof(content.payload->'definitions') <> 'array'
                  OR jsonb_typeof(content.payload->'value') <> 'object'
                  OR content.payload->'value'->>'kind' NOT IN (
                      'scalar', 'package', 'list', 'attribute_set',
                      'submodule', 'opaque', 'failed'
                  )
                  OR NOT (content.payload->'value' ? 'value')
                  OR EXISTS (
                      SELECT 1
                      FROM jsonb_array_elements(
                          CASE WHEN jsonb_typeof(content.payload->'definitions') = 'array'
                               THEN content.payload->'definitions' ELSE '[]'::jsonb END
                      ) definition(value)
                      WHERE jsonb_typeof(definition.value) <> 'object'
                         OR jsonb_typeof(definition.value->'source_path') <> 'string'
                         OR jsonb_typeof(definition.value->'winning') <> 'boolean'
                  ))
          )
    ), corpus_size AS (
        SELECT COUNT(*)::bigint AS value FROM usable_snapshots
    ), paths AS (
        SELECT DISTINCT item.option_path
        FROM usable_snapshots snapshot
        JOIN evaluation_snapshot_options item ON item.snapshot_id = snapshot.id
    ), present_votes AS (
        SELECT item.option_path, item.content_digest, COUNT(*)::bigint AS votes,
               '1:' || encode(item.content_digest, 'hex') AS state_identity
        FROM usable_snapshots snapshot
        JOIN evaluation_snapshot_options item ON item.snapshot_id = snapshot.id
        GROUP BY item.option_path, item.content_digest
    ), votes AS (
        SELECT option_path, content_digest, votes, state_identity
        FROM present_votes
        UNION ALL
        SELECT path.option_path, NULL::bytea,
               corpus.value - COALESCE(present.value, 0), '0:'
        FROM paths path
        CROSS JOIN corpus_size corpus
        LEFT JOIN (
            SELECT option_path, SUM(votes)::bigint AS value
            FROM present_votes
            GROUP BY option_path
        ) present USING (option_path)
        WHERE corpus.value - COALESCE(present.value, 0) > 0
    ), modal AS (
        SELECT option_path, content_digest
        FROM (
            SELECT votes.*,
                   ROW_NUMBER() OVER (
                       PARTITION BY option_path
                       ORDER BY votes DESC, state_identity COLLATE "C"
                   ) AS position
            FROM votes
        ) ranked
        WHERE position = 1
    ), deltas AS (
        SELECT snapshot.id,
               COUNT(*) FILTER (
                   WHERE selected.content_digest IS DISTINCT FROM modal.content_digest
               )::bigint AS host_delta_count
        FROM usable_snapshots snapshot
        LEFT JOIN modal ON true
        LEFT JOIN evaluation_snapshot_options selected
          ON selected.snapshot_id = snapshot.id
         AND selected.option_path = modal.option_path
        GROUP BY snapshot.id
    )
    UPDATE evaluation_snapshots snapshot
    SET host_delta_count = deltas.host_delta_count
    FROM deltas
    WHERE snapshot.id = deltas.id;

END;
$$ LANGUAGE plpgsql;

-- Backfill each commit once. A usable one-configuration commit has no
-- differences from its own modal state and therefore stores zero.
SELECT recompute_evaluation_host_deltas(commit_id)
FROM evaluation_snapshots
GROUP BY commit_id;
