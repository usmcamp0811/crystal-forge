---
id: TASK-506
title: >-
  Niks3 signing-key mismatch: honest validation results, safe agent copy
  diagnostics, OKF troubleshooting
status: Review
assignee:
  - '@agent'
created_date: '2026-10-25 17:03'
updated_date: '2026-10-25 19:03'
labels:
  - cache
  - agent
  - niks3
  - incident
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/339'
priority: high
type: bug
ordinal: 557000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Incident: nix-builder-2 failed to deploy from Niks3 (mTLS, custom CA) with only `Authenticated cache copy failed with status exit status: 1`. TLS, mTLS, narinfo and path-info all worked. Confirmed root cause: the Niks3 signing public key configured in CF was not the key Niks3 signs with (syntactically valid, wrong key). No NixOS/auth/signature-policy change was needed.

Gaps: (1) Niks3 Read Test reports `signing_keys_valid = true` from key presence alone; (2) the agent discards Nix stderr, so a signature failure is indistinguishable from any other failure; (3) the operator guide does not explain signing-key trust or this diagnosis.

Scope is deliberately small: reuse existing discovery/comparison; no cache redesign, no new CI, no automatic key repair, no weakening of require-sigs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Read-only Niks3 test no longer reports signing_keys_valid=true from presence alone; it reports not verified when advertised keys were not obtained
- [x] #2 All-scope test and discovery report an explicit mismatch when no configured key matches a key advertised by Niks3; a matching key is reported as matched
- [x] #3 Format validity, advertised-key match and real artifact signature verification stay distinct in API results and UI/docs wording; no claim of artifact verification where none occurs
- [x] #4 Agent authenticated copy failure reports a fixed safe category (signature/untrusted key, TLS CA, mTLS, missing path, HTTP authorization, other); raw stderr is never logged or returned; unrecognized output stays generic
- [x] #5 Timeout, cancellation, credential lifetime/cleanup and require-sigs are unchanged
- [ ] #6 Regression tests cover wrong-but-valid key, matching key, read-only not-verified, signature-failure diagnostic, secret redaction and the correct-key success path
- [x] #7 docs/knowledge/caches/niks3-cache.md documents signing-key trust, validation boundaries, key rotation and the troubleshooting sequence; nix build .#okf-knowledge passes
- [x] #8 Out-of-scope discoveries recorded as Backlog tasks
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Worktree /home/mcamp/code/crystal-forge/TASK-506-agent-cache-read-auth, branch TASK-506-agent-cache-read-auth, base origin/dev f0dea246. No production host access. An earlier theory (CA/env propagation) was disproved by the owner's diagnosis and its edits were reverted.

MR !339, branch TASK-506-agent-cache-read-auth, commit 21661154. AC6 left unchecked: Read-scope null is covered only by updated retained browser-workflow assertions that were NOT run (fake probe server is plain HTTP, read probe needs HTTPS). Browser exercise of the changed UI not done: preview running but no legitimate login (fixture users have no local password, registration closed). Follow-up: TASK-507. One packaged cf-server-core build failed intermittently on niks3_probe_failure_never_polls_success_transition (builders.rs, untouched); retry passed.
<!-- SECTION:NOTES:END -->
