# TASK-209: Fix cache creation failing with HTTP 500 error

**Status:** To Do  
**Priority:** High  
**Risk:** High  
**Effort:** Small (1-2 hours) - **Root cause confirmed, fix is straightforward**

---

## 🔥 IMMEDIATE FIX (Production Workaround)

**Quick fix to unblock cache creation on `reckless`:**

1. Generate a 32-byte encryption key:
   ```bash
   openssl rand -base64 32
   ```

2. Add to `/glusterfs/shared/campground/fmf-flake/modules/nixos/services/crystal-forge/default.nix`:
   ```nix
   # Around line 900 in the crystal-forge-server service environment
   systemd.services.crystal-forge-server = {
     environment = {
       CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY = "GENERATED_KEY_HERE";
       # OR use existing secret key as fallback
       # (it will use CRYSTAL_FORGE_SECRET_KEY if _CACHE_ENCRYPTION_KEY not set)
     };
   };
   ```

3. Rebuild and restart:
   ```bash
   sudo nixos-rebuild switch --flake .#reckless
   sudo systemctl restart crystal-forge-server
   ```

4. Test cache creation in UI

**Better fix:** Add proper support to FMF wrapper module (see implementation guidance below)

**Why this happened:**
The upstream Crystal Forge NixOS module supports `cfg.cache.encryption_key_file` and exports the key from that file in the server startup script. The FMF wrapper module doesn't expose this option or set the environment variable, so cache secret encryption fails.

---

## Problem

When attempting to add an Attic cache destination through the web UI, the creation fails with:
```
Failed to create destination: HTTP 500: Failed to create cache destination
```

**ROOT CAUSE CONFIRMED:**
Server logs on `reckless` show:
```
ERROR crystal_forge::handlers::api::caches: Failed to create cache destination: missing cache encryption key; set CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY (or CRYSTAL_FORGE_SECRET_KEY)
```

The `CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY` environment variable is **not set** in the production deployment, causing all cache secret encryption to fail.

**User Impact:**
- **CRITICAL:** Development environment (`dev`) is non-functional
- Cannot configure binary caches for build artifact distribution
- Blocks setup of new Crystal Forge instances
- Prevents completion of setup wizard cache step
- No actionable error message shown to user (generic HTTP 500)

**Environment:**
- Occurs in production `dev` environment on `reckless`
- Attempting to add personal Attic cache
- Error appears during UI form submission (Add Cache Destination modal)

**Discovered During:** Attempting to configure dev environment with Attic cache for build artifact distribution

## Goal

1. ~~**Identify root cause** of cache creation failures~~ ✅ **CONFIRMED:** Missing `CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY` environment variable
2. **Fix the deployment configuration** to set the required encryption key environment variable
3. **Improve error reporting** so users see actionable error messages instead of generic HTTP 500
4. **Add startup validation** to detect missing encryption key early (fail-fast instead of runtime errors)
5. **Verify cache creation works** for all cache types (Attic, S3, Nix, Http)

## Non-Goals

- Refactoring the entire cache subsystem architecture
- Changing cache destination database schema
- Migrating existing cache destinations
- Adding new cache types beyond the existing four (S3, Attic, Http, Nix)

## Acceptance Criteria

1. ~~**Root cause identified:**~~ ✅ **DONE**
   - ✅ Server logs confirmed: missing `CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY`
   - Encryption key must be 32 bytes, base64-encoded
   - Can use `CRYSTAL_FORGE_SECRET_KEY` as fallback

2. **Cache creation succeeds:**
   - Can successfully create Attic cache via UI form
   - Cache appears in database with correct encrypted credentials
   - Cache appears in UI cache list immediately after creation
   - Environment assignments (if provided) are correctly stored

3. **Error handling improved:**
   - User sees specific error message explaining what went wrong (not generic "Failed to create cache destination")
   - Server logs include full error context with stack trace
   - Validation errors are caught early and returned as HTTP 400 with field-specific messages
   - Database constraint violations return user-friendly messages

4. **All cache types work:**
   - Attic cache creation succeeds
   - S3 cache creation succeeds
   - Nix cache creation succeeds
   - Http cache creation succeeds

5. **Verification:**
   - Unit tests for cache creation validation pass
   - Integration test: create cache via API, verify in database
   - Manual test: create cache via UI, verify appears in list
   - Server logs show successful creation with debug context

## Architectural Constraints

1. **Maintain existing API contract:**
   - `/api/caches` POST endpoint signature unchanged
   - `CreateCacheDestination` DTO structure unchanged (unless fixing a bug)
   - Response format unchanged (201 Created with redacted cache destination)

2. **Preserve security:**
   - Cache secrets MUST remain encrypted at rest
   - Secrets MUST be redacted from API responses
   - Authentication and authorization checks remain in place (admin-only)

3. **Database integrity:**
   - Transaction-based creation preserved (cache + environment assignments atomic)
   - Foreign key constraints respected (environment_ids must exist)
   - Unique constraints respected (name must be unique)

4. **Backward compatibility:**
   - Existing cache destinations continue to work
   - No breaking changes to cache push jobs
   - No changes to builder cache configuration loading

## Impact Areas

**Primary Investigation:**
- `packages/default/src/handlers/api/caches.rs:140-198` (create_cache_destination handler)
- `packages/default/src/queries/cache_destinations.rs:117-205` (create_cache_destination query)
- `packages/default/src/models/cache_destination.rs:57-86` (CreateCacheDestination validation)
- `packages/default/src/security/cache_secrets.rs` (encryption logic)

**Secondary (if related):**
- Database constraints in `migrations/` (foreign keys, unique constraints)
- UI form validation: `packages/web-ui/src/views/caches.rs:58-140` (validate_cache_destination_form)
- API client: `packages/web-ui/src/api/client.rs:577-583` (create_cache_destination)

**Testing Required:**
- Manual reproduction in local dev environment
- Check server logs for full error trace
- Test all cache types (S3, Attic, Http, Nix)
- Test with/without environment assignments
- Test validation edge cases (empty fields, invalid URLs, missing required fields)

## Verification Plan

### Tier 0: Fast Local Confidence

1. **Code format and linting:**
   ```bash
   nix develop -c cargo fmt --check
   nix develop -c cargo clippy --package crystal-forge -- -D warnings
   ```

2. **Unit tests:**
   ```bash
   nix develop -c cargo test --package crystal-forge cache_destination
   nix develop -c cargo test --package crystal-forge cache_secrets
   ```

3. **Compile check:**
   ```bash
   nix develop -c cargo check --package crystal-forge
   ```

### Tier 1: Feature-Level Integration (REQUIRED)

4. **Reproduce the error locally:**
   ```bash
   nix develop
   full-stack up
   ```
   - Navigate to Caches page
   - Click "Add Destination"
   - Fill in Attic cache details matching production attempt
   - Observe error in UI and check server logs: `journalctl -eu crystal-forge-server -f`
   - **Expected:** See detailed error in logs explaining failure

5. **Test the fix:**
   - Create Attic cache via UI
   - **Expected:** Success, cache appears in list
   - Check database: `psql -c "SELECT id, name, cache_type, enabled FROM cache_destinations;"`
   - **Expected:** New row with encrypted credentials
   
6. **Test all cache types:**
   - Create S3 cache (with access key, secret key)
   - Create Nix cache (HTTP/HTTPS URL)
   - Create Http cache
   - **Expected:** All succeed without HTTP 500

7. **Test error cases:**
   - Submit form with empty name → **Expected:** HTTP 400 with validation error
   - Submit form with duplicate name → **Expected:** HTTP 409 or 400 with "name already exists"
   - Submit form with invalid environment_id → **Expected:** HTTP 400 with "environment not found"
   - Submit form with malformed URL → **Expected:** HTTP 400 with URL validation error

### Success Criteria

- All Tier 0 checks pass
- All Tier 1 manual tests pass
- Cache creation succeeds for all cache types
- Error messages are specific and actionable
- Server logs show detailed context for failures

## Implementation Guidance

### FMF Module Changes Required

**File:** `/glusterfs/shared/campground/fmf-flake/modules/nixos/services/crystal-forge/default.nix`

**Step 1: Add Vault template for cache encryption key (around line 967, after builder.key)**

```nix
"cache-encryption.key" = {
  text = ''
    {{ with secret "${cfg.vault-path}" }}{{ if eq "${cfg.kvVersion}" "v1" }}{{ .Data.cache_encryption_key }}{{ else }}{{ .Data.data.cache_encryption_key }}{{ end }}{{ end }}
  '';
  permissions = "0600";
  change-action = "restart";
};
```

**Step 2: Update setup service to install the key (around line 825, after builder key install)**

```bash
# Install cache encryption key
if [[ -f "$TMPL_DIR/cache-encryption.key" ]]; then
  echo "📦 Installing cache encryption key..."
  timeout 30 sh -c "while [[ ! -s '$TMPL_DIR/cache-encryption.key' ]]; do sleep 0.5; done" \
    || { echo "❌ Timeout waiting for cache-encryption.key"; exit 1; }
  install -D -m 0600 -o crystal-forge -g crystal-forge \
    "$TMPL_DIR/cache-encryption.key" \
    /var/lib/crystal-forge/.config/cache-encryption.key
  echo "✅ Cache encryption key installed"
fi
```

**Step 3: Update server service to export the environment variable (around line 922-934)**

```nix
systemd.services.crystal-forge-server = lib.mkIf cfg.server.enable {
  after = [ "crystal-forge-setup.service" ];
  wants = [ "crystal-forge-setup.service" ];
  serviceConfig = {
    PermissionsStartOnly = true;
    ReadWritePaths = [
      "/var/lib/crystal-forge"
      "/tmp"
      "/run/crystal-forge"
      "/var/cache/crystal-forge-nix"
    ];
    # Add this:
    EnvironmentFile = [ "-/var/lib/crystal-forge/.config/cache-encryption.env" ];
  };
};
```

**Step 4: Create the env file in setup service (alternative to direct export)**

Or better yet, have setup service create an EnvironmentFile:

```bash
# Create cache encryption environment file
if [[ -f "$TMPL_DIR/cache-encryption.key" ]]; then
  echo "📦 Creating cache encryption environment file..."
  timeout 30 sh -c "while [[ ! -s '$TMPL_DIR/cache-encryption.key' ]]; do sleep 0.5; done" \
    || { echo "❌ Timeout waiting for cache-encryption.key"; exit 1; }
  
  KEY_CONTENT=$(cat "$TMPL_DIR/cache-encryption.key")
  echo "CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY=$KEY_CONTENT" > /tmp/cache-encryption.env.tmp
  install -D -m 0600 -o crystal-forge -g crystal-forge \
    /tmp/cache-encryption.env.tmp \
    /var/lib/crystal-forge/.config/cache-encryption.env
  rm /tmp/cache-encryption.env.tmp
  echo "✅ Cache encryption environment file created"
fi
```

**Step 5: Add Vault secret**

In your Vault KV store at the path configured in `cfg.vault-path`, add:
```
cache_encryption_key = "<32-byte base64 key>"
```

Generate with: `openssl rand -base64 32`

### ROOT CAUSE CONFIRMED ✅

**Missing encryption key environment variable in FMF wrapper:**

The FMF module at `/glusterfs/shared/campground/fmf-flake/modules/nixos/services/crystal-forge/default.nix` doesn't:
1. Expose the `cache.encryption_key_file` option (upstream module has this at line 937-942)
2. Export `CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY` in the server startup (upstream does this at lines 234-241)

**Solution:** Add Vault-backed encryption key support to FMF module using the same pattern as agent keys

**Possibility 2: Foreign Key Constraint (environment_ids)**
If UI sends `environment_ids` that don't exist in database, transaction will fail.
- Fix: Validate environment IDs exist before insertion
- Improve: Return HTTP 400 with specific environment IDs that failed
- Error message: "Environment not found: [list of invalid IDs]"

**Possibility 3: Validation Error Not Caught**
If validation passes in UI but fails on server, error isn't properly returned.
- Fix: Ensure `create.validate()` errors are properly mapped to HTTP 400
- Check: Line 159-169 in `handlers/api/caches.rs`

**Possibility 4: Attic-Specific Field Missing**
Attic requires `attic_cache_name` and optionally `attic_public_key`.
- Fix: Validate Attic-specific fields are present when cache_type is "Attic"
- Check: `CreateCacheDestination::validate()` implementation

### Code Locations to Review

**Handler (error response):**
`packages/default/src/handlers/api/caches.rs:175-196`
```rust
Err(e) => {
    tracing::error!("Failed to create cache destination: {:#}", e);
    // ^^ Check logs for this error
    let error_msg = if e.to_string().contains("duplicate key") { ... }
    // Add more specific error detection here
}
```

**Query (database insert):**
`packages/default/src/queries/cache_destinations.rs:136-188`
```rust
let destination = sqlx::query_as::<_, CacheDestination>(...).await?;
// ^^ Check if this fails

// Assign environments if provided
if let Some(ref env_ids) = create.environment_ids {
    for env_id in env_ids {
        // ^^ Check if environment_id references are valid
    }
}
```

**Validation:**
`packages/default/src/models/cache_destination.rs` (check the `validate()` method)

**Encryption:**
`packages/default/src/security/cache_secrets.rs` (check `encrypt_optional()`)

### Error Handling Improvements

Add specific error types and improve user-facing messages:

```rust
// In handlers/api/caches.rs create_cache_destination
Err(e) => {
    let error_str = e.to_string();
    tracing::error!("Failed to create cache destination: {:#}", e);
    
    let (status, error_msg) = if error_str.contains("duplicate key") {
        (StatusCode::CONFLICT, format!("Cache '{}' already exists", create.name))
    } else if error_str.contains("foreign key") {
        (StatusCode::BAD_REQUEST, "One or more environment IDs are invalid".to_string())
    } else if error_str.contains("encryption") {
        (StatusCode::INTERNAL_SERVER_ERROR, "Failed to encrypt cache secrets".to_string())
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create cache: {}", e))
    };
    
    (status, Json(ApiError { 
        error: "cache_creation_failed".to_string(),
        message: error_msg,
        details: Some(error_str), // Include technical details for debugging
    })).into_response()
}
```

### Testing Checklist

After fix is implemented:

- [ ] Reproduce original error locally
- [ ] Apply fix
- [ ] Verify fix resolves original error
- [ ] Test Attic cache creation
- [ ] Test S3 cache creation
- [ ] Test Nix cache creation
- [ ] Test Http cache creation
- [ ] Test with environment assignments
- [ ] Test without environment assignments
- [ ] Test validation errors return HTTP 400
- [ ] Test duplicate name returns appropriate error
- [ ] Test invalid environment_id returns HTTP 400
- [ ] Check server logs show detailed error context
- [ ] Deploy to `dev` environment and verify

## Dependencies

None - this is a critical bug fix for core functionality.

## Follow-up Tasks

After this fix is deployed:

- Consider adding integration tests for cache creation (all types)
- Consider adding startup validation for `CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY`
- Consider adding cache destination health checks (test connection to cache URLs)
- Document required Attic configuration in setup wizard or help text

## Notes

- This is blocking normal development workflow in `dev` environment
- The generic HTTP 500 error makes debugging difficult for users
- Improving error messages will help users self-diagnose issues
- Check if this affects cache creation via API directly (not just UI)
- Verify existing cache destinations (if any) still work after fix

## Debugging Commands

**Check server logs for error:**
```bash
# On reckless (production)
journalctl -eu crystal-forge-server -n 500 | grep -i "cache"

# Local dev
# Check process-compose output when reproducing
```

**Check database state:**
```bash
# Connect to database
nix develop -c psql

# Check existing cache destinations
SELECT id, name, cache_type, enabled, created_at FROM cache_destinations;

# Check environment assignments
SELECT cd.name, e.name as environment, cde.created_at 
FROM cache_destination_environments cde
JOIN cache_destinations cd ON cd.id = cde.cache_destination_id
JOIN environments e ON e.id = cde.environment_id;

# Check environments exist
SELECT id, name FROM environments;
```

**Check encryption key:**
```bash
# Verify environment variable is set
echo $CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY | base64 -d | wc -c
# Should output: 32
```

**Test API directly (bypassing UI):**
```bash
# Get auth token
TOKEN=$(curl -s http://localhost:3444/api/auth/dev/login | jq -r .token)

# Create cache via API
curl -X POST http://localhost:3444/api/caches \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "test-cache",
    "cache_type": "Attic",
    "push_to": "https://attic.example.com/test",
    "attic_cache_name": "test",
    "attic_token": "test-token",
    "enabled": true
  }'
```

## Related Tasks

- Setup wizard cache step (requires working cache creation)
- Builder cache configuration (depends on cache destinations)
- TASK-208 (UI key format bug - separate issue)
