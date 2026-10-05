---
type: Historical Reference
title: "TASK-412 Implementation Records (retained Backlog documents)"
description: "Points to the two retained Backlog documents (doc-20 and doc-21) that record TASK-412 Slice 2 and Slices 1 to 5: transactional trust and publication atomicity, digest validation, audit events, and the tests and caveats of that work."
tags:
  - crystal-forge
  - task-412
  - compliance
  - xccdf
  - audit
implementation_status: historical
status: deprecated
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:58:48-05:00
sources:
  - id: s1
    resource: "Crystal Forge repository file backlog/docs/doc-20%20-%20TASK-412-Slice-2-Implementation-Summary.md at commit 3b23d36f"
    title: "TASK-412 Slice 2 Implementation Summary"
  - id: s2
    resource: "Crystal Forge repository file backlog/docs/doc-21%20-%20TASK-412-Complete-Implementation-Slice-1-5-Verification-Summary.md at commit 3b23d36f"
    title: "TASK-412 Complete Implementation - Slices 1-5 Verification Summary"
---
# TASK-412 Implementation Records (retained Backlog documents)

> **Status:** historical. These are task records from August 2026 (created 2026-08-10) for branch `TASK-412-cf-xccdf-interchange`, MR !313. doc-20 is the earlier snapshot ("Core implementation complete ... Remaining: test updates and formal verification"). doc-21 is the later one ("Implementation complete. All slices delivered and committed."). The records are kept for their reasoning about transactional trust and publication. They are not an operator guide.

## Retained files

| File | Role |
| --- | --- |
| [doc-20 - TASK-412-Slice-2-Implementation-Summary.md](<../../../backlog/docs/doc-20 - TASK-412-Slice-2-Implementation-Summary.md>) (132 lines) | Slice 2 handler rewrites, helpers, implementation notes, and what remained. |
| [doc-21 - TASK-412-Complete-Implementation-Slice-1-5-Verification-Summary.md](<../../../backlog/docs/doc-21 - TASK-412-Complete-Implementation-Slice-1-5-Verification-Summary.md>) (187 lines) | Slices 1 to 5, design requirements met, known limitations, commit timeline, and reviewer steps. |

Backlog.md manages both files by ID, so they stay in place.

## What the documents record

- Slice 1 (foundation): CF-XCCDF schema and parser, migrations `0195` to `0210`, policy and bundle versioning, import/export endpoints with digest validation.
- Slice 2 (commit `345f2e6b`): four handlers made transactional and auditable (`trust_policy_version`, `trust_bundle_version`, `publish_policy_version`, `publish_bundle_version`) with `FOR UPDATE` locks, digest recomputation that rejects `pending` and stale digests, and audit events written in the same transaction.
- Slices 3 and 4 (commits `238e7987`, `2365aa72`): publish tests pre-trust versions, plus 11 audit and atomicity tests (A to K), all marked `--ignored` because they need a live database.
- Design points: native, external, and manual implementations must be trusted before publication; trust and publish are separate operations; a failed validation rolls back with no partial state; publication uses a trigger-safe pointer sequence (clear draft, accept, set published pointer).
- Known limitations recorded in doc-21: leftover test data in the dev database, a digest backfill for trigger-created `pending` digests, no multi-connection concurrency tests, and no per-member audit event when a bundle publish auto-publishes draft members.

## Implementation status and evidence

The handlers exist in `packages/default/crates/cf-server/src/handlers/api/compliance.rs`: `trust_policy_version`, `trust_bundle_version`, `publish_policy_version`, `publish_bundle_version`, `write_audit_event`, and locked digest helpers (`recompute_policy_version_digest_locked`, `recompute_bundle_version_digest_locked`, `apply_policy_publication_locked`). The records name helpers without the `_locked` suffix and cite `src/compliance/digest.rs`, so the code has been refactored since. The records' file and line references are stale.

## Related concepts

- [CF-XCCDF interchange operator guide](../compliance/cf-xccdf-interchange-operator-guide.md): current operator-facing trust and publication behavior.
- [CF-XCCDF interchange profile](../compliance/cf-xccdf-interchange-profile.md): the design this work implements.
