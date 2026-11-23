-- Reasserts the complete access-token scope allowlist (TASK-534.9).
--
-- COMPATIBILITY: 0343 (compliance scopes) and 0346 (poams:create) both replace
-- `api_access_token_scopes_scope_check`. The two migrations reached persistent
-- databases from different branches, so they can run in either order:
--
-- * 0343 before 0346 is the numbered order. 0346 leaves all 14 scopes.
-- * 0346 before 0343 happens on a database that ran TASK-534.9 before its
--   branch contained 0343. SQLx applies the missing lower version 0343 later,
--   and 0343 removes `poams:create` from the allowlist.
--
-- This migration has the highest version of both branches, so it runs last in
-- every order and leaves the same final constraint. It does not change token
-- rows. Applied migrations 0343 and 0346 stay unchanged because SQLx rejects a
-- changed checksum.

ALTER TABLE api_access_token_scopes
    DROP CONSTRAINT IF EXISTS api_access_token_scopes_scope_check;

ALTER TABLE api_access_token_scopes
    ADD CONSTRAINT api_access_token_scopes_scope_check
    CHECK (scope IN ('cves:read','poams:read','provenance:read','scans:request',
                     'poams:write','poams:verify','poams:close','poams:create',
                     'policies:read','policies:draft','policies:import',
                     'bundles:read','bundles:draft','compliance:preview'));
