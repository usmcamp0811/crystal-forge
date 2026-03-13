---
id: TASK-144
title: 'BLOCKER: Fix cryptographically broken Ed25519 keypair generation'
status: Done
assignee: []
created_date: '2026-03-01 02:27'
updated_date: '2026-03-13 01:24'
labels:
  - security
  - blocker
  - crypto
  - backend
  - web-ui
dependencies: []
priority: high
ordinal: 86000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The current builder keypair generation in the UI uses browser crypto to generate random bytes for both public and private keys **independently**. This is cryptographically incorrect for Ed25519:

- Public key MUST be derived from the private key (not generated separately)
- Current implementation breaks signature verification
- Signatures won't verify because the stored "public key" doesn't correspond to the private key
- Trains users to use broken keys (security issue)

**Current broken code location**: `packages/web-ui/src/views/builders_list.rs` - generate_keypair_browser()

## Security Impact

- **Critical**: Authentication will fail - signatures can't verify against mismatched public keys
- **Critical**: Trains developers/operators to use cryptographically invalid keys
- **Blocker for merge**: This must be fixed before the multi-builder API can be released

## Solution Options

### Option 1: Server-side generation (RECOMMENDED)

**Why better**: 
- Single source of truth for keypair generation
- Proper Ed25519 library usage guaranteed
- No client-side crypto complexity

**Implementation**:
1. Add POST /api/v1/builders/{id}/regenerate-keypair endpoint (admin only)
2. Server generates Ed25519 keypair using `ed25519-dalek` or `ring`
3. Store only public_key in database
4. Return private_key ONCE in response (never stored, never logged)
5. UI displays private key in modal with "copy to clipboard" + warning "save this now, you won't see it again"
6. After modal dismissed, private key is cleared from memory

### Option 2: Client-side generation with proper library

**Implementation**:
1. Use WASM-friendly Ed25519 library (e.g., `ed25519-dalek` via wasm-bindgen)
2. Generate private key from CSPRNG
3. **Derive** public key from private key (not generate separately!)
4. Send only public_key to server for storage
5. User copies private_key from UI (never sent to server)

**Tradeoffs**: More complex client bundle, but keeps private keys client-side only

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Ed25519 public key is mathematically derived from private key (not randomly generated)
- [ ] #2 Signature verification works: sign with private key → verify with stored public key succeeds
- [ ] #3 Private key is never stored in database
- [ ] #4 Private key is shown to user exactly once at creation time
- [ ] #5 Test added: generate keypair, sign message, verify signature with stored public key
- [ ] #6 Documentation updated to reflect secure keypair generation

## References

- Ed25519 spec: public key = SHA512(private_key)[32:64] (simplified)
- Libraries: `ed25519-dalek`, `ring`, `libsodium` all implement this correctly
<!-- SECTION:DESCRIPTION:END -->

<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Reality check (2026-03-01): implemented and merged into dev.

Implemented server-side Ed25519 keypair generation and regenerate-keypair endpoint; public key is derived from private key; private key is returned once and not persisted.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented and merged: cryptographically-correct Ed25519 keypair flow for builders.

Server now generates/rotates keypairs correctly, stores only public key, and returns private key one-time at create/regenerate endpoints.
<!-- SECTION:FINAL_SUMMARY:END -->
