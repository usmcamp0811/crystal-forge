---
id: TASK-367
title: >-
  INFRA: Replace MinIO with secure S3-compatible alternative in test
  infrastructure
status: To Do
assignee: []
created_date: '2026-06-23 22:10'
updated_date: '2026-06-23 22:24'
labels:
  - infrastructure
  - testing
  - security
  - s3
  - ci
dependencies: []
priority: high
ordinal: 319000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Web-UI integration tests fail because MinIO has been marked insecure in nixpkgs:

```
Refusing to evaluate package 'minio-2025-10-15T17-29-55Z' because it is marked as insecure

Known issues:
- CVE-2026-40344: Unauthenticated Object Write via Missing Signature Verification
- CVE-2026-41145: Unauthenticated Object Write via Query-String Credential Signature Bypass
- CVE-2026-33322: JWT Algorithm Confusion in OIDC Authentication
- CVE-2026-33419: LDAP login brute-force via user enumeration
- CVE-2026-34204: SSE Metadata Injection via Replication Headers
- CVE-2026-39414: DoS via Unbounded Memory Allocation in S3 Select CSV Parsing
- MinIO has been abandoned by upstream and security issues won't be fixed
```

This is blocking web-ui checks in CI.

## Goal

Migrate test infrastructure from MinIO to a maintained, secure S3-compatible alternative.

## Recommended Alternatives

1. **Garage** - Rust-based, actively maintained, lightweight
2. **SeaweedFS** - Go-based, good performance, active development
3. **Ceph RGW** - Enterprise-grade, heavier weight

Prefer Garage for test VMs due to lightweight footprint and Nix/Rust ecosystem alignment.

## Migration Path

1. Replace MinIO service in test VM configuration with chosen alternative
2. Update S3 client configuration for any endpoint/credential differences
3. Verify web-ui tests pass with new S3 backend
4. Document the change in test infrastructure docs

## Impact

- `checks/web-ui/` - test VM configuration
- Any S3 client setup in test fixtures
<!-- SECTION:DESCRIPTION:END -->
