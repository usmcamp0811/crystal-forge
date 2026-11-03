-- Exact-reuse admission accepts a completed static cache publication.
--
-- derivation_publication_eligible(source, output, environment), from 0322, accepts
-- only `database` publications and unambiguous `legacy` ones. A deployment that
-- publishes through the server's static cache configuration (`cache.push_to` with
-- `cache.push_after_build`, no database destination) records `static`
-- publications, which never qualified. An identical derivation then built again,
-- and a reused head could not be scanned.
--
-- The 4-argument overload adds one acceptance rule and keeps the 3-argument one
-- as is, so database and legacy behavior is unchanged:
--
--   a completed `static` publication of exactly this output whose recorded
--   destination equals `static_destination`.
--
-- `static_destination` is the destination of the CURRENTLY EFFECTIVE static
-- configuration: `cache.push_to` when `cache.push_after_build` is true, else NULL.
-- The database cannot read the server's configuration, so the caller passes it.
-- NULL or an empty value accepts no static publication. A publication recorded for
-- another destination, or made while publication was disabled and then never
-- completed, is therefore not accepted. The rule mirrors the existing static
-- checks in cache_push.rs (select_job_destination) and cve_worker.rs
-- (materialization_source_for_reference).
--
-- SECURITY: This is an admission predicate only. It returns a boolean, never a
-- credential, and it does not authorize execution. A scan still needs its own
-- claim (producing session, environment, capability), the server-local worker
-- re-resolves the static source against the configuration in force when it runs,
-- and delivery to an agent resolves publications through
-- resolve_publication_read_tx, which has no static source and so withholds a static
-- publication. A historical publication grants nothing on its own.
CREATE FUNCTION derivation_publication_eligible(
    source_id integer, output_path text, subject_environment uuid, static_destination text
) RETURNS boolean LANGUAGE sql STABLE AS $$
    SELECT derivation_publication_eligible(source_id, output_path, subject_environment)
        OR (
            NULLIF(btrim(static_destination), '') IS NOT NULL
            AND EXISTS (
                SELECT 1 FROM cache_push_jobs push
                WHERE push.derivation_id = source_id AND push.store_path = output_path
                  AND push.status = 'completed' AND push.completed_at IS NOT NULL
                  AND push.cache_destination_source = 'static'
                  AND push.cache_destination_id IS NULL
                  AND push.cache_destination = static_destination
            )
        )
$$;

COMMENT ON FUNCTION derivation_publication_eligible(integer, text, uuid, text) IS
    'Admission-only publication proof: the 3-argument database/legacy rules, or a completed static publication of this exact output to static_destination (the currently effective static push_to, NULL when publication is disabled). Returns no credential and authorizes no execution.';
