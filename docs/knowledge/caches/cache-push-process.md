---
type: Workflow
title: "Cache Push Process"
description: "Describes cache types (Nix, S3, Attic, HTTP), cache push features, and cache push job semantics; open it when configuring or debugging pushes to a binary cache."
tags:
  - crystal-forge
  - caches
  - cache-push
  - s3
  - attic
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:15-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/derivation-status.md at commit 3b23d36f"
    title: "Crystal Forge Derivation Status Flow"
---

# Cache Push Process

> **Status:** Split from the derivation status document. Cache backends are defined in `packages/default/crates/cf-config/src/config/cache.rs` and `packages/default/crates/cf-server/src/builder/cache_worker.rs`. Feature details (filtering, signing, parallel uploads) were not compared with the code and are verification candidates.

## **Cache Types**
- **Nix**: Standard nix copy to HTTP/S3 endpoints
- **S3**: Direct S3 upload with optional signing
- **Attic**: Specialized Nix binary cache with token auth
- **HTTP**: Generic HTTP-based binary cache

## **Cache Push Features**
- **Filtering**: Only push derivations matching configured patterns
- **Parallel uploads**: Configurable concurrent push operations
- **Retry logic**: Exponential backoff for failed pushes
- **Signing**: Optional store path signing for security

## **Cache Push Jobs**
- Queued automatically after successful builds
- Tracked separately from derivation status
- Include metadata: size, duration, error messages
- Support both `.drv` paths and resolved store paths

## Related concepts

- [Derivation processing loops](../architecture/derivation-processing-loops.md) - the cache push loop
- [Derivation status lifecycle](../concepts/derivation-status-lifecycle.md) - the cache-pushed status
- [Commit to deploy sequence](../workflows/commit-eval-build-cache-deploy-sequence.md) - idempotent cache push in the sequence
