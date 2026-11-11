-- Resolves the one environment that owns a derivation's build and publication.
--
-- A configuration may be registered several times. Registrations in ONE
-- environment resolve to that environment. Equally preferred registrations in
-- DIFFERENT environments are ambiguous, and the function reports that instead
-- of choosing one. Callers MUST NOT substitute a NULL (global) environment for
-- an ambiguous result: a NULL build-job environment is claimable by every
-- builder, and a NULL publication environment selects global destinations.
--
-- Matching rules (single source of truth for build admission, builder claims,
-- cache publication and exact-artifact reuse):
--   * only active registrations in the commit's flake count;
--   * registrations without an environment are ignored;
--   * a system_configuration_name match (rank 0) beats a hostname match (rank 1);
--   * only the best rank is considered.
--
-- Returns exactly one row. No match, or a NULL commit, yields (NULL, false).
CREATE FUNCTION derivation_environment(p_commit_id integer, p_name text)
RETURNS TABLE (environment_id uuid, ambiguous boolean)
LANGUAGE sql STABLE AS $$
    WITH matches AS (
        SELECT s.environment_id,
               CASE WHEN NULLIF(s.system_configuration_name, '') = p_name THEN 0 ELSE 1 END AS preference
        FROM systems s
        JOIN commits c ON c.flake_id = s.flake_id
        WHERE c.id = p_commit_id
          AND s.environment_id IS NOT NULL
          AND s.is_active
          AND (s.hostname = p_name OR NULLIF(s.system_configuration_name, '') = p_name)
    ), preferred AS (
        SELECT DISTINCT m.environment_id
        FROM matches m
        WHERE m.preference = (SELECT MIN(preference) FROM matches)
    )
    SELECT CASE WHEN (SELECT COUNT(*) FROM preferred) = 1
                THEN (SELECT environment_id FROM preferred) END,
           (SELECT COUNT(*) FROM preferred) > 1
$$;
