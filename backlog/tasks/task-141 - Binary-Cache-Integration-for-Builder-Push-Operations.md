---
id: TASK-141
title: Binary Cache Integration for Builder Push Operations
status: Backlog
assignee: []
created_date: '2026-03-01 14:01'
labels:
  - backend
  - builder
  - cache
  - security
  - infrastructure
milestone: m-15
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Builders currently have no configured binary cache destinations to push build outputs to after successfully building NixOS configurations. This means:

- Built derivations are not shared across builders or systems
- Repeated builds waste resources rebuilding identical outputs
- No centralized artifact storage for deployments
- Missing cache authentication/credentials infrastructure

## Goal

Enable builders to push successfully built derivations to configured binary caches (e.g., S3, Cachix, Attic, or local HTTP caches) with proper authentication, scoped to environments.

Crystal Forge does NOT implement the cache itself — it manages the credentials and configuration needed for builders to authenticate and push to external cache services.

## Scope

### Backend

1. **Cache Registry Data Model**
   - Cache entries with name, type (s3/cachix/attic/http), endpoint URL
   - Credential storage (secret keys, tokens, credentials encrypted at rest)
   - Environment assignment (1:many - cache can serve multiple environments)

2. **API Endpoints**
   - `GET/POST /api/v1/caches` - List/create cache configurations
   - `GET/PATCH/DELETE /api/v1/caches/:id` - Get/update/delete cache
   - `PATCH /api/v1/caches/:id/environments` - Assign cache to environments
   - `GET /api/v1/caches/:id/credentials` - Retrieve credentials (builder-auth only)

3. **Builder Integration**
   - Builder queries assigned cache for its current build job's environment
   - Builder receives cache endpoint + credentials via API
   - Builder configures `nix copy` / `nix-store --push` with provided credentials
   - Builder pushes successful builds to cache after completion

4. **Security**
   - Cache credentials encrypted at rest in database
   - Only builder-authenticated endpoints can retrieve credentials
   - Admin-only cache management endpoints
   - Audit logging for cache credential access

### Frontend

1. **Cache Management View**
   - Admin-accessible "Caches" section (or tab under Server Management)
   - List view: cache name, type, endpoint, environments, status
   - Add/edit/delete cache configurations
   - Environment assignment UI (multi-select)
   - Credential input (secret key/token with masked display)

2. **Environment Integration**
   - Show assigned cache on environment detail/edit views
   - Allow cache assignment during environment creation

### Database Schema

```sql
CREATE TABLE binary_caches (
    id UUID PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    cache_type TEXT NOT NULL, -- 's3', 'cachix', 'attic', 'http'
    endpoint_url TEXT NOT NULL,
    -- Encrypted credentials (type-specific JSON)
    credentials_encrypted BYTEA NOT NULL,
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE cache_environment_assignments (
    id SERIAL PRIMARY KEY,
    cache_id UUID REFERENCES binary_caches(id) ON DELETE CASCADE,
    environment_id UUID REFERENCES environments(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(cache_id, environment_id)
);

CREATE INDEX idx_cache_env_assignments ON cache_environment_assignments(environment_id);
```

### Credential Types (JSON stored encrypted)

**S3-compatible:**
```json
{
  "access_key_id": "...",
  "secret_access_key": "...",
  "region": "us-east-1",
  "bucket": "my-nix-cache"
}
```

**Cachix:**
```json
{
  "auth_token": "...",
  "cache_name": "my-cache"
}
```

**Attic:**
```json
{
  "endpoint": "https://attic.example.com",
  "token": "..."
}
```

**HTTP (with optional auth):**
```json
{
  "endpoint": "https://cache.example.com",
  "auth_header": "Bearer ..." // optional
}
```

## Implementation Phases

### Phase 1: Data Model & Encryption
- Create database tables and migrations
- Implement credential encryption/decryption (using app-level encryption key)
- Create models and DTOs

### Phase 2: Backend API
- Implement cache CRUD endpoints (admin-only)
- Implement environment assignment endpoints
- Implement builder-auth credential retrieval endpoint
- Add audit logging for credential access

### Phase 3: Builder Integration
- Modify builder to query cache for current job's environment
- Implement credential injection into Nix environment
- Add `nix copy` or push logic after successful build
- Handle cache push failures gracefully (log but don't fail build)

### Phase 4: Frontend UI
- Create cache management view with list/add/edit/delete
- Add environment assignment UI
- Implement credential input with masking
- Add cache status indicators

### Phase 5: Testing & Documentation
- Unit tests for encryption/decryption
- Integration tests for builder cache push flow
- Documentation for setting up external caches (S3, Cachix, Attic)
- Security audit for credential storage

## Non-Goals

- Implementing a binary cache server (use external services)
- Cache garbage collection (handled by cache service)
- Cache replication or mirroring
- Public cache serving (caches are builder push targets only)
- Automatic cache selection (explicit environment assignment only)

## Security Considerations

- Credentials MUST be encrypted at rest
- Credentials MUST only be retrievable by authenticated builders
- Credentials MUST NOT appear in logs or error messages
- Admin UI MUST mask credential values after initial entry
- Audit log MUST record credential access events

## Acceptance Criteria
<!-- AC:BEGIN -->
See AC section below.

## Risk Level

High (credential management, builder integration)
<!-- SECTION:DESCRIPTION:END -->

- [ ] #1 Database schema created with binary_caches and cache_environment_assignments tables
- [ ] #2 Credential encryption/decryption implemented with secure key management
- [ ] #3 Admin API endpoints for cache CRUD operations implemented
- [ ] #4 Environment assignment API for caches implemented
- [ ] #5 Builder-authenticated endpoint to retrieve cache credentials for job environment
- [ ] #6 Builder integration: query cache for current job environment
- [ ] #7 Builder integration: configure Nix with cache credentials
- [ ] #8 Builder integration: push successful builds to assigned cache
- [ ] #9 Cache push failures logged but do not fail the build job
- [ ] #10 Frontend cache management view (list/add/edit/delete) implemented
- [ ] #11 Frontend environment assignment UI for caches implemented
- [ ] #12 Frontend credential input with masked display after entry
- [ ] #13 Admin-only authorization enforced on cache management endpoints
- [ ] #14 Audit logging for cache credential access events
- [ ] #15 Unit tests for credential encryption/decryption
- [ ] #16 Integration tests for builder cache push workflow
- [ ] #17 Documentation for setting up external cache services (S3, Cachix, Attic)
- [ ] #18 Security review completed for credential storage and access patterns
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 All API endpoints tested (unit + integration)
- [ ] #2 Builder can successfully push to S3-compatible cache
- [ ] #3 Builder can successfully push to Cachix
- [ ] #4 Frontend cache management UI tested
- [ ] #5 Credential encryption verified secure
- [ ] #6 Audit events captured for credential access
- [ ] #7 cargo fmt and cargo clippy pass
- [ ] #8 Documentation includes cache setup examples
- [ ] #9 No credentials appear in logs or error messages
<!-- DOD:END -->
