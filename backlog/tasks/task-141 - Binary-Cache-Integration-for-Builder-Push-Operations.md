---
id: TASK-141
title: Binary Cache Integration for Builder Push Operations
status: Backlog
assignee: []
created_date: '2026-03-01 14:01'
updated_date: '2026-03-01 14:04'
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

Builders currently support pushing to binary caches (S3, Attic, HTTP/Nix), but cache configuration is managed via TOML config files. This means:

- Cache credentials are in plaintext config files (security risk)
- No environment-scoped cache assignment (all builds use same cache)
- No admin UI to manage cache configurations
- Cache settings require file edits and restarts to change
- No audit trail for cache credential access

**Existing Infrastructure:**
- ✅ `cache_push_jobs` table for tracking push operations
- ✅ Builder cache push worker loops (`run_cache_push_loop`)
- ✅ Support for S3, Attic, HTTP, and Nix cache types
- ✅ Retry logic with exponential backoff
- ✅ Store path signing support
- ✅ `CacheConfig` with all necessary fields

## Goal

Move cache configuration from file-based to database-backed with admin UI, enabling:

1. **Environment-scoped cache assignment** - Each environment can have its own cache
2. **Secure credential storage** - Encrypt cache credentials at rest in database
3. **Admin UI** - Manage caches without editing config files
4. **Audit logging** - Track cache credential access by builders
5. **Dynamic updates** - Change cache configs without restarting builders

Crystal Forge does NOT implement the cache itself — it manages the credentials and configuration needed for builders to authenticate and push to external cache services.

## Current Implementation (To Preserve)

### Existing Code
- `packages/default/src/config/cache.rs` - CacheConfig with S3/Attic/HTTP support
- `packages/default/src/queries/cache_push.rs` - Cache push job queries
- `packages/default/src/builder/mod.rs` - Cache push worker loops
- `packages/default/src/derivations/cache.rs` - Push logic with retry
- `cache_push_jobs` table - Job tracking (has `cache_destination` field!)

### Cache Types Already Supported
- **S3**: `nix copy --to s3://...` with region/profile
- **Attic**: `attic push <cache> <path>` with token
- **HTTP/Nix**: `nix copy --to https://...`

## Scope

### Phase 1: Database Migration (Extend Existing Tables)

Add cache registry table:
```sql
CREATE TABLE binary_caches (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT UNIQUE NOT NULL,
    cache_type TEXT NOT NULL, -- 's3', 'attic', 'http', 'nix'
    endpoint_url TEXT NOT NULL,
    -- Encrypted credentials (JSON matching CacheConfig fields)
    credentials_encrypted BYTEA NOT NULL,
    -- Cache configuration
    push_after_build BOOLEAN DEFAULT true,
    signing_key_path TEXT,
    compression TEXT, -- 'xz', 'bzip2', 'gzip', etc.
    force_repush BOOLEAN DEFAULT false,
    max_retries INTEGER DEFAULT 3,
    retry_delay_seconds INTEGER DEFAULT 5,
    push_timeout_seconds BIGINT DEFAULT 3600,
    -- S3 specific
    s3_region TEXT,
    s3_profile TEXT,
    parallel_uploads INTEGER DEFAULT 1,
    -- Attic specific
    attic_cache_name TEXT,
    attic_ignore_upstream_cache_filter BOOLEAN DEFAULT true,
    attic_jobs INTEGER DEFAULT 5,
    -- Status
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

### Phase 2: Backend - Models & Encryption

1. **Credential Encryption**
   - Use app-level encryption key (store in env var or secrets manager)
   - Encrypt JSON blob containing type-specific credentials
   - Decrypt only when builder requests credentials

2. **Models**
   - `BinaryCache` struct matching table
   - `BinaryCacheCredentials` enum for type-specific creds:
     - S3: access_key_id, secret_access_key
     - Attic: token
     - HTTP: optional auth_header
   - `BinaryCacheWithEnvironments` for detail view

3. **Queries**
   - CRUD operations for binary_caches
   - Environment assignment queries
   - `get_cache_for_environment(env_id) -> Option<BinaryCache>`

### Phase 3: Backend - API Endpoints

**Admin endpoints:**
- `GET/POST /api/v1/caches` - List/create
- `GET/PATCH/DELETE /api/v1/caches/:id` - Detail/update/delete
- `PATCH /api/v1/caches/:id/environments` - Assign to environments

**Builder-authenticated endpoint:**
- `GET /api/v1/build-jobs/:job_id/cache-config` - Get cache for job's environment
  - Returns decrypted `CacheConfig` struct (matches existing code)
  - Only accessible by authenticated builder
  - Audit logged

### Phase 4: Builder Integration

**Modify existing builder code:**

1. **Current flow (file-based):**
   ```rust
   let cache_config = cfg.get_cache_config(); // From TOML
   process_cache_pushes(&pool, cache_config, build_config).await;
   ```

2. **New flow (DB-backed with file fallback):**
   ```rust
   // Try DB first, fallback to TOML config
   let cache_config = if let Some(job) = current_job {
       api_client.get_cache_config_for_job(job.id).await?
   } else {
       cfg.get_cache_config().clone() // Legacy fallback
   };
   process_cache_pushes(&pool, &cache_config, build_config).await;
   ```

3. **Update `cache_push_jobs.cache_destination`:**
   - Populate with cache name/ID from environment assignment
   - Use for tracking which cache a job targets

### Phase 5: Frontend - Cache Management UI

**Admin > Caches section:**

1. **List View**
   - Table: Name, Type, Endpoint, Environments, Status
   - Add Cache button
   - Edit/Delete actions

2. **Add/Edit Modal**
   - Name input
   - Type selector (S3 / Attic / HTTP / Nix)
   - Endpoint URL
   - Type-specific credential fields (masked after save):
     - S3: Access Key ID, Secret Access Key, Region, Bucket
     - Attic: Token, Cache Name
     - HTTP: Optional Auth Header
   - Environment multi-select
   - Advanced options: compression, signing key, retry config
   - Test connection button (optional)

3. **Environment View Enhancement**
   - Show assigned cache on environment detail
   - Allow cache selection during environment create/edit

### Phase 6: Migration Path

1. **Backward compatibility:**
   - Keep TOML-based `CacheConfig` as fallback
   - If no DB cache found for environment, use TOML config
   - Allow gradual migration

2. **Migration helper:**
   - Script to import existing TOML cache config into DB
   - `nix develop -c cargo run --bin import-cache-config`

## Implementation Phases

1. **Phase 1**: Database schema + migration
2. **Phase 2**: Credential encryption + models + queries
3. **Phase 3**: Admin API endpoints + builder API endpoint
4. **Phase 4**: Builder integration (DB-first, TOML fallback)
5. **Phase 5**: Frontend cache management UI
6. **Phase 6**: Testing + migration script + documentation

## Non-Goals

- Implementing a binary cache server (use external services)
- Cache garbage collection (handled by cache service)
- Cache replication or mirroring
- Public cache serving (caches are builder push targets only)
- Removing TOML config support (keep as fallback)

## Security Considerations

- Credentials MUST be encrypted at rest using AES-256-GCM or similar
- Encryption key stored in environment variable or secrets manager
- Credentials MUST only be retrievable by authenticated builders
- Credentials MUST NOT appear in logs or error messages
- Admin UI MUST mask credential values after initial entry
- Audit log MUST record credential access events

## Risk Level

High (credential management, builder integration, backward compatibility)
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



## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Database schema created with binary_caches and cache_environment_assignments tables
- [ ] #2 Credential encryption/decryption implemented with secure key management (AES-256-GCM or similar)
- [ ] #3 Admin API endpoints for cache CRUD operations implemented
- [ ] #4 Environment assignment API for caches implemented
- [ ] #5 Builder API endpoint to retrieve CacheConfig for job environment
- [ ] #6 Builder integration: query cache from DB for current job, fallback to TOML config if not found
- [ ] #7 Builder integration: existing cache push logic continues to work with DB-sourced config
- [ ] #8 Cache push failures logged but do not fail the build job (existing behavior preserved)
- [ ] #9 Frontend cache management view (list/add/edit/delete) implemented
- [ ] #10 Frontend environment assignment UI for caches implemented
- [ ] #11 Frontend credential input with type-specific fields and masked display after entry
- [ ] #12 Admin-only authorization enforced on cache management endpoints
- [ ] #13 Builder-auth-only enforcement on cache config retrieval endpoint
- [ ] #14 Audit logging for cache credential access events
- [ ] #15 Unit tests for credential encryption/decryption
- [ ] #16 Integration tests for DB-backed cache config retrieval by builder
- [ ] #17 Migration script to import existing TOML cache config into database
- [ ] #18 Documentation for cache setup via UI and migration from TOML config
- [ ] #19 Backward compatibility: TOML config works as fallback when no DB cache assigned
<!-- AC:END -->
