-- Preserve publication identity after cache deletion. No foreign key is used:
-- ON DELETE SET NULL would let a database job fall back to static credentials.
ALTER TABLE cache_push_jobs
    ADD COLUMN cache_destination_id INTEGER,
    ADD COLUMN cache_destination_source TEXT NOT NULL DEFAULT 'legacy',
    ADD CONSTRAINT cache_push_destination_provenance CHECK (
        (cache_destination_source = 'database' AND cache_destination_id IS NOT NULL)
        OR (cache_destination_source IN ('static', 'legacy') AND cache_destination_id IS NULL)
    );

COMMENT ON COLUMN cache_push_jobs.cache_destination_id IS
    'Durable selected database destination identity; retained after deletion. Never resolve a missing ID by name, URL or static configuration.';
COMMENT ON COLUMN cache_push_jobs.cache_destination_source IS
    'database: exact ID; static: explicit static producer; legacy: uncertain historical reference requiring unambiguous eligible name/URL resolution before publication.';

-- Existing rows remain legacy. A URL or NULL cannot prove static provenance.
-- Workers pin unambiguous eligible legacy names/URLs to a database ID before
-- publication. Legacy rows with no matching database destination fail closed;
-- administrators must explicitly establish static provenance before retrying
-- an uncertain historical static job. The migration cannot infer that intent.
