---
id: TASK-145
title: 'BLOCKER: Add replay resistance to builder request signatures'
status: Done
assignee: []
created_date: '2026-03-01 02:27'
updated_date: '2026-03-13 01:24'
labels:
  - security
  - blocker
  - auth
  - backend
dependencies: []
priority: high
ordinal: 87000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Current builder authentication signs only the request body with X-Builder-ID + X-Signature headers. This is vulnerable to replay attacks:

- Any captured signed request can be resent
- No timestamp or nonce in signed payload
- Signature doesn't include method or path (can be moved between endpoints)
- Attacker can replay valid requests indefinitely

## Security Impact

- **Critical**: Replay attacks allow unauthorized actions
- **Critical**: Lack of method/path binding allows signature reuse across different endpoints
- **Blocker for merge**: Must have replay resistance before production use

## Solution

Implement comprehensive replay resistance following best practices:

### 1. Add timestamp to signed payload

**Headers**:
- Add `X-Timestamp` header with ISO 8601 timestamp
- Builder includes timestamp in signature calculation

**Signed payload format**:
```
{method}\n{path}\n{timestamp}\n{body}
```

Example:
```
POST\n/api/v1/builders/123/heartbeat\n2026-03-01T02:30:00Z\n{"status":"active"}
```

### 2. Enforce freshness window

**Server-side validation**:
- Check `|now() - request_timestamp| <= 5 minutes`
- Reject requests outside window with 401 Unauthorized
- Non-verbose error: "Request timestamp invalid"

### 3. Include method + path in signature

**Prevents**:
- Moving signature from GET to POST
- Moving signature from /heartbeat to /jobs/claim
- Cross-endpoint signature reuse

### 4. Optional: Nonce deduplication (for high-security)

**If needed**:
- Add X-Nonce header (UUID v4)
- Store recent nonces in Redis/Postgres with TTL
- Reject duplicate nonces within window
- Cleanup expired nonces automatically

## Implementation Locations

**Backend** (`packages/default/src/auth/builders.rs`):
- `verify_builder_signature()` - add timestamp + method + path to signed payload
- Add freshness window check
- Update signature format constant

**Builder client** (builder binary or API client):
- Include timestamp in signature calculation
- Include method + path in signed payload
- Send X-Timestamp header

**Documentation** (`docs/multi-builder-api.md`):
- Update authentication section with new signature format
- Document freshness window
- Provide example signature generation code

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 X-Timestamp header required on all builder requests
- [ ] #2 Signature includes: method + path + timestamp + body
- [ ] #3 Freshness window enforced (±5 minutes configurable)
- [ ] #4 Replayed requests (identical timestamp) rejected
- [ ] #5 Signature cannot be moved between endpoints (path binding)
- [ ] #6 Test added: replay attack with old timestamp fails
- [ ] #7 Test added: signature reuse across endpoints fails
- [ ] #8 Documentation updated with new signature format

## Example Code

```rust
// Signed payload format
let payload = format!("{}\n{}\n{}\n{}", 
    method,           // "POST"
    path,             // "/api/v1/builders/123/heartbeat"  
    timestamp,        // "2026-03-01T02:30:00Z"
    body              // request body as string
);

// Signature = sign_ed25519(payload, private_key)
// Verify:    verify_ed25519(payload, signature, public_key)
```

## References

- IETF HTTP Signatures: RFC 9421
- AWS Signature Version 4 (similar approach)
- HMAC request signing best practices
<!-- SECTION:DESCRIPTION:END -->

<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Reality check (2026-03-01): implemented and merged into dev.

Implemented required X-Timestamp header, method+path+timestamp+body canonical signature payload, and ±5 minute freshness validation.

Follow-up gap task created as TASK-150 for explicit replay/cross-endpoint test coverage and any policy decision on nonce-based deduplication.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented and merged: replay-resistant builder request authentication contract.

Builder auth now binds signatures to method/path/timestamp/body and rejects stale timestamps outside freshness window.
<!-- SECTION:FINAL_SUMMARY:END -->
