---
id: TASK-148
title: 'HIGH PRIORITY: Add public key validation for builder registration'
status: Done
assignee: []
created_date: '2026-03-01 02:29'
updated_date: '2026-03-13 01:24'
labels:
  - security
  - high-priority
  - backend
  - validation
dependencies: []
priority: high
ordinal: 90000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The `builders.public_key` column is TEXT, which allows invalid data:

- No validation that it's valid base64
- No validation of Ed25519 public key length (32 bytes)
- No rejection of oversized inputs
- Allows garbage data that will fail signature verification

## Security Impact

- **High**: Invalid public keys cause authentication failures
- **High**: Allows DoS via huge public key strings (database bloat, memory exhaustion)
- **Impact**: Confusing errors when signature verification fails with invalid keys

## Solution

Add aggressive validation at registration time:

### 1. Base64 decode validation

```rust
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

fn validate_public_key(public_key: &str) -> Result<Vec<u8>, ValidationError> {
    // 1. Decode base64
    let decoded = BASE64.decode(public_key)
        .map_err(|_| ValidationError::InvalidBase64)?;
    
    // 2. Check Ed25519 public key length (32 bytes)
    if decoded.len() != 32 {
        return Err(ValidationError::InvalidKeyLength {
            expected: 32,
            got: decoded.len(),
        });
    }
    
    // 3. Optional: Validate it's a valid curve point
    // (Some Ed25519 libraries provide point validation)
    
    Ok(decoded)
}
```

### 2. Request body size limits

**Axum layer config**:
```rust
// Limit builder registration request to 1 MB max
.layer(RequestBodyLimitLayer::new(1024 * 1024))
```

**Per-field validation**:
```rust
#[derive(Deserialize, Validate)]
struct CreateBuilderRequest {
    #[validate(length(min = 1, max = 1000))]  // Reasonable limit
    name: String,
    
    #[validate(length(min = 1, max = 1000))]  // Base64(32 bytes) = 44 chars + padding
    public_key: String,
}
```

### 3. Reject oversized inputs early

**In handler**:
```rust
pub async fn create_builder(
    State(pool): State<PgPool>,
    Json(payload): Json<CreateBuilderRequest>,
) -> Result<Json<Builder>, ApiError> {
    // Validate BEFORE database query
    payload.validate()
        .map_err(|e| ApiError::BadRequest(format!("Validation failed: {}", e)))?;
    
    // Validate public key specifically
    let _decoded_key = validate_public_key(&payload.public_key)
        .map_err(|e| ApiError::BadRequest(format!("Invalid public key: {}", e)))?;
    
    // Proceed with creation...
}
```

### 4. Database constraint (defense in depth)

**Migration**:
```sql
-- Add CHECK constraint on public_key length
ALTER TABLE builders
ADD CONSTRAINT builders_public_key_length_check
CHECK (LENGTH(public_key) <= 1000);
```

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Public key validated as valid base64 at registration
- [ ] #2 Public key decoded length validated (32 bytes for Ed25519)
- [ ] #3 Registration rejected with clear error for invalid base64
- [ ] #4 Registration rejected with clear error for wrong key length
- [ ] #5 Request body size limited (prevent huge payloads)
- [ ] #6 Database constraint prevents oversized keys
- [ ] #7 Test added: invalid base64 rejected with 400
- [ ] #8 Test added: wrong length key rejected with 400
- [ ] #9 Test added: oversized request body rejected with 413
- [ ] #10 Documentation updated with public key format requirements

## Implementation Locations

- `packages/default/src/handlers/api/builders.rs` - `create_builder()` endpoint
- `packages/default/src/api/models.rs` - Add validation to `CreateBuilderRequest`
- `packages/default/src/auth/builders.rs` - Add `validate_public_key()` helper
- `packages/default/migrations/0086_add_public_key_constraints.sql` (new)

## Error Messages

**Good (actionable)**:
```
400 Bad Request: Invalid public key: failed to decode base64
400 Bad Request: Invalid public key: expected 32 bytes, got 64
413 Payload Too Large: Request body exceeds 1 MB limit
```

**Bad (non-actionable)**:
```
400 Bad Request: Invalid input
500 Internal Server Error
```

## Example Test

```rust
#[tokio::test]
async fn test_invalid_public_key_rejected() {
    let client = TestClient::new().await;
    
    let response = client.post("/api/v1/builders")
        .json(&json!({
            "name": "test-builder",
            "public_key": "not-valid-base64!!!"  // Invalid
        }))
        .send()
        .await;
    
    assert_eq!(response.status(), 400);
    assert!(response.text().contains("Invalid public key"));
}
```
<!-- SECTION:DESCRIPTION:END -->

<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Reality check (2026-03-01): implemented and merged into dev.

Added public-key base64/length/curve validation path, request validation improvements, DB constraints migration (0086), and invalid-key tests.

Follow-up gap task created as TASK-150 for explicit oversized-body 413 test/docs parity against original acceptance text.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented and merged: builder public key validation and DB guardrails.

Registration now validates key format/size and rejects invalid key material with actionable errors.
<!-- SECTION:FINAL_SUMMARY:END -->
