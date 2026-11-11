-- Replaces the 0330 resolver so that ambiguity is decided per effective
-- configuration identity.
--
-- Defect in 0330: it ranked an explicit system_configuration_name match above
-- a hostname match and examined only the best rank. A reference registration
-- with system_configuration_name = 'web' in environment A hid a legacy managed
-- registration with hostname 'web' (no explicit name) in environment B. Both
-- registrations select nixosConfigurations.web, but 0330 returned A as the
-- unique owner. 0330 also matched a hostname of a registration whose explicit
-- name selects a different configuration.
--
-- Rule (single source of truth for build admission, builder claims, cache
-- publication and exact-artifact reuse):
--   * only active registrations in the commit's flake count;
--   * registrations without an environment are ignored;
--   * the effective configuration identity of a registration is its trimmed,
--     non-empty system_configuration_name, else its hostname (legacy
--     fallback). This is the identity that admission already uses;
--   * every registration whose effective identity equals p_name counts,
--     regardless of whether the identity is explicit or a fallback;
--   * registrations in one environment resolve to that environment; several
--     environments are ambiguous.
--
-- Callers MUST NOT substitute a NULL (global) environment for an ambiguous
-- result: a NULL build-job environment is claimable by every builder, and a
-- NULL publication environment selects global destinations.
--
-- Returns exactly one row. No match, or a NULL commit, yields (NULL, false).
CREATE OR REPLACE FUNCTION derivation_environment(p_commit_id integer, p_name text)
RETURNS TABLE (environment_id uuid, ambiguous boolean)
LANGUAGE sql STABLE AS $$
    WITH owners AS (
        SELECT DISTINCT s.environment_id
        FROM systems s
        JOIN commits c ON c.flake_id = s.flake_id
        WHERE c.id = p_commit_id
          AND s.environment_id IS NOT NULL
          AND s.is_active
          AND COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname) = p_name
    )
    SELECT CASE WHEN (SELECT COUNT(*) FROM owners) = 1
                THEN (SELECT environment_id FROM owners) END,
           (SELECT COUNT(*) FROM owners) > 1
$$;
