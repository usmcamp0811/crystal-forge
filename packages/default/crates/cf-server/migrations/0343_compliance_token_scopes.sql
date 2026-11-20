-- Compliance access-token scopes (TASK-534.6).
--
-- Replaces the scope allowlist of migration 0336 with the seven existing
-- scopes plus the six compliance scopes. `api_access_token_scopes_scope_check`
-- is the name PostgreSQL gave the unnamed column CHECK in 0336. The service
-- catalog decides which scopes an owner may select; this constraint only
-- rejects names that no release knows.

ALTER TABLE api_access_token_scopes
    DROP CONSTRAINT IF EXISTS api_access_token_scopes_scope_check;

ALTER TABLE api_access_token_scopes
    ADD CONSTRAINT api_access_token_scopes_scope_check CHECK (
        scope IN (
            'cves:read',
            'poams:read',
            'provenance:read',
            'scans:request',
            'poams:write',
            'poams:verify',
            'poams:close',
            'policies:read',
            'policies:draft',
            'policies:import',
            'bundles:read',
            'bundles:draft',
            'compliance:preview'
        )
    );
