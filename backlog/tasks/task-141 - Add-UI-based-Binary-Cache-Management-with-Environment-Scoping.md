---
id: TASK-141
title: Add UI-based Binary Cache Management with Environment Scoping
status: In Progress
assignee: []
created_date: '2026-03-01 14:01'
updated_date: '2026-03-01 18:52'
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
<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Crystal Forge **already supports binary cache push** via TOML configuration (`cache.push_to`, `cache.cache_type`, etc.), but:

- **No environment-scoped caches** - All builds use the same cache regardless of environment (dev/staging/prod may need different caches)
- **No UI management** - Cache settings require editing TOML files and restarting services
- **Credentials in plaintext** - Cache tokens/keys live in config files (security concern for production)
- **No audit trail** - No visibility into which builds pushed to which caches

**Current State (Working):**
- ✅ Full cache push infrastructure implemented
- ✅ S3, Attic, HTTP/Nix cache support working
- ✅ `cache_push_jobs` table tracking all pushes
- ✅ Retry logic, signing, compression all functional
- ✅ TOML-based `CacheConfig` struct used everywhere

## Goal

Add **database-backed cache registry with admin UI** while maintaining full backward compatibility with TOML config.

**Enable:**
1. **Environment-scoped caching** - Dev pushes to dev cache, prod to prod cache
2. **UI-based management** - Admin can add/edit caches without file access
3. **Secure credential storage** - Encrypt sensitive cache credentials in DB
4. **Audit logging** - Track which environments/builds use which caches

**Preserve:**
- TOML-based cache config continues to work as-is
- Existing builder code requires minimal changes
- No breaking changes to current deployments

## Configuration Hierarchy (Industry Standard Approach)

Follow **12-Factor App** and **Kubernetes ConfigMap/Secret** patterns:

### Precedence (Highest to Lowest)

1. **Database (per-environment)** - Most specific, wins if present
2. **TOML file** - Global defaults/fallback
3. **Code defaults** - Built-in sensible defaults

### Rules

- **DB overrides TOML when present** for that environment
- **TOML remains fallback** when no DB cache assigned to environment
- **No DB writes from TOML** - DB is source of truth for UI-managed configs
- **TOML is read-only** - Builder never writes back to TOML from DB state

### Implementation

```rust
async fn get_cache_config(
    pool: &PgPool,
    environment_id: Option<Uuid>,
    toml_config: &CacheConfig,
) -> CacheConfig {
    // Try DB first (environment-specific)
    if let Some(env_id) = environment_id {
        if let Ok(Some(db_cache)) = get_cache_for_environment(pool, env_id).await {
            return db_cache.to_cache_config(); // DB wins
        }
    }
    
    // Fall back to TOML (global default)
    toml_config.clone()
}
```

### Example Scenarios

**Scenario 1: Pure TOML (existing deployments)**
- No DB caches configured
- All environments use TOML config
- Works exactly as it does today ✅

**Scenario 2: Mixed Mode (gradual migration)**
- Prod environment has DB cache assigned → uses DB config
- Dev/staging have no DB cache → use TOML config
- Both work simultaneously ✅

**Scenario 3: Full UI Management**
- All environments have DB caches assigned
- TOML cache config ignored (but still valid as fallback)
- Admin manages everything via UI ✅

**Scenario 4: DB Deletion**
- Admin deletes DB cache assignment
- Environment automatically falls back to TOML config
- No service restart needed ✅

## Scope

### Phase 1: Database Schema

```sql
CREATE TABLE binary_caches (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT UNIQUE NOT NULL,
    cache_type TEXT NOT NULL CHECK (cache_type IN ('s3', 'attic', 'http', 'nix')),
    
    -- Common settings (match CacheConfig struct)
    push_to TEXT, -- endpoint URL or s3://bucket
    push_after_build BOOLEAN DEFAULT true,
    signing_key_path TEXT,
    compression TEXT,
    force_repush BOOLEAN DEFAULT false,
    max_retries INTEGER DEFAULT 3,
    retry_delay_seconds INTEGER DEFAULT 5,
    push_timeout_seconds BIGINT DEFAULT 3600,
    
    -- S3-specific
    s3_region TEXT,
    s3_profile TEXT,
    parallel_uploads INTEGER DEFAULT 1,
    
    -- Attic-specific  
    attic_cache_name TEXT,
    attic_ignore_upstream_cache_filter BOOLEAN DEFAULT true,
    attic_jobs INTEGER DEFAULT 5,
    
    -- Encrypted credentials (type-specific JSON)
    credentials_encrypted BYTEA,
    
    is_active BOOLEAN DEFAULT true,
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
```

### Phase 2: Backend - Config Resolution

**Key function (minimal change to builder):**

```rust
// In builder binary startup or job execution
let cache_config = resolve_cache_config(
    &pool,
    job.environment_id, // Option<Uuid>
    &toml_config.cache, // &CacheConfig from file
).await;

// Existing code continues unchanged
process_cache_pushes(&pool, &cache_config, build_config).await;
```

**New queries:**
- `get_cache_for_environment(pool, env_id) -> Option<BinaryCache>`
- `decrypt_credentials(encrypted_blob) -> BinaryCacheCredentials`
- `BinaryCache::to_cache_config() -> CacheConfig` (conversion)

### Phase 3: Backend - API Endpoints

**Admin endpoints (require admin role):**
- `GET /api/v1/caches` - List all caches
- `POST /api/v1/caches` - Create cache
- `GET /api/v1/caches/:id` - Get cache details
- `PATCH /api/v1/caches/:id` - Update cache
- `DELETE /api/v1/caches/:id` - Delete cache
- `PATCH /api/v1/caches/:id/environments` - Assign to environments

**Builder endpoints (require builder auth):**
- `GET /api/v1/environments/:id/cache-config` - Get resolved config for environment
  - Returns decrypted `CacheConfig` ready for use
  - Falls back to indicating "use TOML" if no DB cache

### Phase 4: Frontend - Admin UI

**Admin > Server Management > Caches**

1. **List View**
   - Table: Name, Type, Endpoint, Environments (count), Status
   - "Add Cache" button
   - Edit/Delete actions per row

2. **Add/Edit Cache Modal**
   - **Basic Info:** Name, Cache Type (dropdown)
   - **Configuration** (type-specific forms):
     - S3: Endpoint, Region, Bucket, Access Key ID, Secret Key (masked)
     - Attic: Cache Name, Token (masked)
     - HTTP/Nix: Endpoint URL, Optional Auth Header (masked)
   - **Advanced:** Compression, Signing Key Path, Retry Settings
   - **Environment Assignment:** Multi-select dropdown
   - Save encrypts credentials before storing

3. **Environment Detail Enhancement**
   - Show "Assigned Cache: {name}" or "Using global TOML config"
   - Quick-assign cache from environment view

### Phase 5: Credential Encryption

**Approach: Application-Level Encryption**

```rust
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::Rng;

// Encryption key from env var (32 bytes for AES-256)
const CACHE_ENCRYPTION_KEY: &str = env!("CRYSTAL_FORGE_CACHE_KEY");

fn encrypt_credentials(plaintext: &str) -> Vec<u8> {
    // AES-256-GCM with random nonce
    // Store: nonce (12 bytes) + ciphertext + tag (16 bytes)
}

fn decrypt_credentials(blob: &[u8]) -> Result<String> {
    // Extract nonce, decrypt, verify tag
}
```

**Key management:**
- Stored in environment variable or secrets manager
- Rotated via re-encryption migration (future task)
- Never logged or exposed via API

### Phase 6: Migration & Compatibility

**No migration needed** - existing TOML configs continue working.

**Optional helper script:**
```bash
# Import current TOML cache into DB for specific environment
nix develop -c crystal-forge-admin import-cache \
  --environment prod \
  --from-toml /var/lib/crystal_forge/config.toml
```

## Non-Goals

- ❌ Removing TOML cache support
- ❌ Automatic DB ↔ TOML sync (one-way: DB overrides)
- ❌ Implementing cache server itself
- ❌ Cache garbage collection
- ❌ Multi-region cache replication

## Security

- Credentials encrypted with AES-256-GCM
- Encryption key in env var (not in code/DB)
- Credentials only decrypted for authenticated builders
- Admin UI masks credentials after entry
- Audit log records all credential access
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Database schema created with binary_caches and cache_environment_assignments tables
- [ ] #2 Credential encryption/decryption implemented with AES-256-GCM
- [ ] #3 Cache config resolution logic: DB overrides TOML when present, TOML is fallback
- [ ] #4 Admin API endpoints for cache CRUD operations implemented
- [ ] #5 Environment assignment API for caches implemented
- [ ] #6 Builder integration: resolve_cache_config() uses DB first, then TOML fallback
- [ ] #7 Existing TOML-only deployments continue working without any DB configuration
- [ ] #8 Builder can use DB-sourced cache config for assigned environment
- [ ] #9 Builder falls back to TOML cache config when no DB cache assigned
- [ ] #10 Frontend cache management view (list/add/edit/delete) implemented
- [ ] #11 Frontend environment assignment UI for caches implemented
- [ ] #12 Frontend credential input with type-specific fields and masked display
- [ ] #13 Admin-only authorization enforced on cache management endpoints
- [ ] #14 Audit logging for cache credential access by builders
- [ ] #15 Unit tests for credential encryption/decryption
- [ ] #16 Unit tests for config resolution hierarchy (DB > TOML > default)
- [ ] #17 Integration test: environment with DB cache uses DB config
- [ ] #18 Integration test: environment without DB cache uses TOML config
- [ ] #19 Documentation explains TOML vs DB precedence and migration path
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Adjust TopLayout user dropdown container width so it is slightly wider and unaffected by ineffective w-100 utility; prefer explicit min-width while preserving existing menu layout.

Verify the class update in topbar component compiles and does not alter unrelated layout behavior.

Run targeted web-ui check/tests under nix develop for confidence before continuing merge prep.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: gpt-5.3-codex on gray in ~/code/crystal-forge/TASK-141-themeable-web-ui-css-extraction

Adjusted TopLayout user dropdown width by replacing `w-100` with `min-w-[240px]` in `packages/web-ui/src/components/layout/topbar.rs` so menu can grow wider than trigger width.

Verification: `nix develop -c cargo check` (in `packages/web-ui`) passed; `nix develop -c cargo test` (in `packages/web-ui`) passed (36 passed, 0 failed, 1 ignored).

Initial `nix develop -c cargo check -p web-ui` from repo root failed because no root Cargo.toml; reran from `packages/web-ui` successfully.

User reported first width adjustment had no visible effect. Updated dropdown to use a standard utility plus inline style fallback (`w-64` and `min-width: 16rem`) in `packages/web-ui/src/components/layout/topbar.rs` to avoid CSS extraction misses for arbitrary utility classes.

Re-verified build with `nix develop -c cargo check` in `packages/web-ui` (passes with existing unrelated warnings).
<!-- SECTION:NOTES:END -->

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
