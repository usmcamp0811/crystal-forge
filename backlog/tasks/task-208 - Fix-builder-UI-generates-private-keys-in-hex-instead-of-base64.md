# TASK-208: Fix builder UI generates private keys in hex instead of base64

**Status:** Backlog  
**Priority:** High  
**Risk:** Medium  
**Effort:** Small (2-4 hours)

## Problem

The builder keypair generation UI returns Ed25519 private keys encoded as 64-character hex strings, but the builder runtime expects base64-encoded 32-byte seeds. This format mismatch causes authentication failures when users copy private keys from the UI into their deployment configuration (NixOS modules, Vault secrets, etc.).

**User Impact:**
- Users cannot successfully use builder API mode with keys generated in the UI
- Manual hex-to-base64 conversion is required as a workaround
- Blocks adoption of the builder API mode feature
- Creates confusion about the correct key format

**Root Cause:**
The UI's `generate_ed25519_keypair()` function in `packages/web-ui/src/components/builders/keypair_generator.rs:22` encodes the private key as hex:
```rust
private_key: hex::encode(keypair.secret.as_bytes()),
```

The builder runtime in `packages/default/src/builder/api_client.rs:67-84` expects base64:
```rust
let decoded = BASE64_STANDARD.decode(key)?;
if decoded.len() != 32 {
    return Err(/* ... */);
}
```

**Discovered During:** Implementation of builder API mode Vault support for FMF Crystal Forge module (reckless system configuration)

## Goal

Ensure the builder keypair generation UI returns private keys in the same base64 format that:
1. The builder runtime expects for authentication
2. The server API's keypair generation endpoint uses (`packages/default/src/queries/builders.rs:30`)
3. Can be directly copied into deployment configuration without conversion

## Non-Goals

- Changing the public key format (hex is appropriate for display)
- Modifying the builder runtime's key loading logic (base64 is the correct format)
- Supporting automatic migration of existing hex keys in the database
- Adding key format detection/conversion in the builder (keys should be stored correctly)

## Acceptance Criteria

1. **UI keypair generation returns base64 private keys:**
   - `packages/web-ui/src/components/builders/keypair_generator.rs` `generate_ed25519_keypair()` function returns `private_key` as base64-encoded 32 bytes
   - Format matches server API's `generate_builder_keypair()` in `packages/default/src/queries/builders.rs:30`

2. **All UI keypair generation flows updated:**
   - "Generate New Keypair" button in Add Builder modal (`add_builder_modal.rs:35-43`)
   - "Rotate Key" button in Edit Builder modal (`edit_builder_modal.rs:106-114`)
   - Any other UI locations that call `generate_ed25519_keypair()`

3. **Public key remains hex-encoded:**
   - Public keys should continue to use hex format (appropriate for display and database storage)
   - Only private key format changes

4. **Generated keys work with builder runtime:**
   - Private keys copied from UI can be used directly in builder configuration
   - Builder authenticates successfully with server API using UI-generated keys
   - No manual format conversion required

5. **Code consistency:**
   - All keypair generation logic uses the same encoding (base64 for private, hex for public)
   - No mixing of formats within the codebase

## Architectural Constraints

1. **Maintain compatibility with server API format:**
   - The server's `generate_builder_keypair()` already returns base64 private keys
   - UI must match this format for consistency

2. **No database schema changes:**
   - Private keys are stored as text in the database
   - Both hex and base64 are valid text storage
   - Existing keys will remain in their current format (migration out of scope)

3. **No breaking changes to builder runtime:**
   - Builder already expects base64
   - This fix makes UI match the expected format

4. **Preserve existing public key format:**
   - Public keys should remain hex-encoded for display purposes
   - Database stores public keys as hex

## Impact Areas

**Modified Components:**
- `packages/web-ui/src/components/builders/keypair_generator.rs` (primary fix)
- `packages/web-ui/src/components/builders/add_builder_modal.rs` (verification)
- `packages/web-ui/src/components/builders/edit_builder_modal.rs` (verification)

**Testing Required:**
- Unit tests for `generate_ed25519_keypair()` function
- Manual verification:
  - Generate keypair in Add Builder modal
  - Copy private key and verify it's valid base64 (not hex)
  - Verify length: base64-encoded 32 bytes should be 44 characters (with padding)
  - Test builder authentication with UI-generated key

**Dependencies:**
- `ed25519-dalek` crate (already in use)
- `base64` crate (already in use)
- No new dependencies required

## Verification Plan

### Tier 0: Fast Local Confidence

1. **Code format and linting:**
   ```bash
   nix develop -c cargo fmt --check
   nix develop -c cargo clippy --package crystal-forge-web-ui -- -D warnings
   ```

2. **Unit tests (if added):**
   ```bash
   nix develop -c cargo test --package crystal-forge-web-ui keypair
   ```

3. **Compile check:**
   ```bash
   nix develop -c cargo check --package crystal-forge-web-ui
   ```

### Tier 1: Feature-Level Integration (REQUIRED)

4. **Manual UI testing:**
   ```bash
   # Start full stack
   nix develop
   full-stack up
   ```
   
   Then verify:
   - Navigate to Builders page
   - Click "Add Builder"
   - Click "Generate New Keypair"
   - **Expected:** Private key is 44 characters (base64 with padding) or 43 (without)
   - **Expected:** Private key contains only base64 chars: A-Z, a-z, 0-9, +, /, =
   - **Not expected:** Private key is 64 characters (hex)
   - Copy private key
   - Decode: `echo "<private_key>" | base64 -d | wc -c` should output `32`
   
   - Test "Rotate Key" in Edit Builder modal
   - Verify same format requirements

5. **Runtime authentication test:**
   - Create a test builder in UI with generated keypair
   - Copy private key to a test configuration
   - Start builder in API mode pointing to local server
   - **Expected:** Builder authenticates successfully
   - **Expected:** No "Invalid key format" errors in logs

### Success Criteria

- All Tier 0 checks pass
- All Tier 1 manual verification steps pass
- Private keys are base64-encoded 32-byte values
- Public keys remain hex-encoded
- Builder runtime accepts UI-generated private keys without conversion

## Implementation Guidance

### Primary Change Location

`packages/web-ui/src/components/builders/keypair_generator.rs:22`

**Current (incorrect):**
```rust
private_key: hex::encode(keypair.secret.as_bytes()),
```

**Should be:**
```rust
private_key: BASE64_STANDARD.encode(keypair.secret.as_bytes()),
```

### Reference Implementation

The server API already does this correctly in `packages/default/src/queries/builders.rs:30`:
```rust
let private_key_b64 = BASE64_STANDARD.encode(keypair.secret.as_bytes());
```

### Verification Points

- `ed25519_dalek::SigningKey::as_bytes()` returns 32 bytes
- `BASE64_STANDARD.encode()` of 32 bytes produces 44 characters (with padding)
- Hex encoding of 32 bytes produces 64 characters
- The builder runtime expects exactly 32 bytes after base64 decode

### Related Files to Review

- `packages/web-ui/src/components/builders/add_builder_modal.rs:35-43` (calls keypair generator)
- `packages/web-ui/src/components/builders/edit_builder_modal.rs:106-114` (calls keypair generator)
- `packages/default/src/builder/api_client.rs:67-84` (runtime validation reference)

## Dependencies

None - this is an isolated UI fix.

## Follow-up Tasks

After this fix is merged, consider:
- Adding validation in the UI to detect and warn about hex-formatted keys in existing configs
- Documentation update explaining the correct key format for manual generation
- Migration script for users who have hex keys in their database (out of scope for this task)

## Notes

- This bug was discovered during FMF module builder API mode integration
- The workaround is: `echo "<hex_key>" | xxd -r -p | base64`
- The server API has always generated correct base64 keys
- Only the UI keypair generator was affected
- Existing builders with hex keys in the database will need manual migration (separate task if needed)

## Related Tasks

- Builder API mode Vault support (completed, awaiting deployment)
- Reckless system builder configuration (blocked by this bug)
