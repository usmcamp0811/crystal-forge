-- Resolves the one environment that owns a derivation's build and publication.
--
-- A configuration may be registered several times. Registrations in ONE
-- environment resolve to that environment. Registrations of the SAME
-- configuration in DIFFERENT environments are ambiguous, and the function
-- reports that instead of choosing one. Callers MUST NOT substitute a NULL
-- (global) environment for an ambiguous result: a NULL build-job environment
-- is claimable by every builder, and a NULL publication environment selects
-- global destinations.
--
-- Matching rules (single source of truth for build admission, builder claims,
-- cache publication and exact-artifact reuse):
--   * only active registrations in the commit's flake count;
--   * registrations without an environment are ignored;
--   * each registration's effective configuration name is its explicit
--     system_configuration_name if set, otherwise its hostname (legacy);
--   * all registrations whose effective name matches p_name are considered,
--     regardless of whether they used explicit or legacy hostname fallback;
--   * ambiguity is detected across ALL matching registrations, not only
--     within one match type.
--
-- Returns exactly one row. No match, or a NULL commit, yields (NULL, false).
CREATE FUNCTION derivation_environment(p_commit_id integer, p_name text)
RETURNS TABLE (environment_id uuid, ambiguous boolean)
LANGUAGE sql STABLE AS $$
    WITH effective_configs AS (
        SELECT s.environment_id,
               COALESCE(NULLIF(s.system_configuration_name, ''), s.hostname) AS config_name
        FROM systems s
        JOIN commits c ON c.flake_id = s.flake_id
        WHERE c.id = p_commit_id
          AND s.environment_id IS NOT NULL
          AND s.is_active
    ), matches AS (
        SELECT DISTINCT environment_id
        FROM effective_configs
        WHERE config_name = p_name
    )
    SELECT CASE WHEN (SELECT COUNT(*) FROM matches) = 1
                THEN (SELECT environment_id FROM matches) END,
           (SELECT COUNT(*) FROM matches) > 1
$$;
