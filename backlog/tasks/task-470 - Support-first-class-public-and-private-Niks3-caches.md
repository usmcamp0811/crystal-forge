---
id: TASK-470
title: Support first-class public and private Niks3 caches
status: To Do
assignee: []
created_date: '2026-10-02 22:23'
labels: []
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/329'
  - 'https://github.com/Mic92/niks3/tree/v1.6.0'
priority: high
type: feature
ordinal: 491000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement the user's Niks3 contract reviewed against MR !329. Niks3 has separate write control and Nix read planes. Private agent cache reads currently lack credential transport. User authorized implementation from origin/dev and an MR after verification; preserve untracked dev/session-ses_f927.md. Initial scope is static-token or mTLS writes and public or mTLS reads. External providers and Bearer/OIDC reads are deferred. Do not refactor generic push_to naming or cache priorities.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Niks3 is a distinct validated cache type across configuration and protocol; unknown types fail closed
- [ ] #2 Separate write/API URL and read/substituter URL with multiple signing keys are persisted using a new migration
- [ ] #3 Tokens and private keys are encrypted and redacted; configured flags and atomic credential replacement and clearing are supported
- [ ] #4 Remote builder and local cache worker push through packaged Niks3 CLI with protected temporary files and no secrets in argv or logs
- [ ] #5 Builder secrets and agent read secrets require verified confidential transport; agents receive only environment-scoped read credentials
- [ ] #6 Server independently verifies publication using the selected destination identity and authenticated Nix read configuration
- [ ] #7 Agent deployment and CVE materialization support public and mTLS reads with signature verification and credential cleanup
- [ ] #8 Discovery and structured non-mutating connection tests enforce existing SSRF and redirect policy
- [ ] #9 Design and production UI support discovery and separate read/write authentication for create and edit with loading and failure states
- [ ] #10 Focused unit tests and Niks3 VM tests cover remote builder plus agent public/private flows environment isolation CVE materialization and secret-log audits
- [ ] #11 Packaging preserves evaluator Nix selection; required server builder Niks3 Attic S3 web-ui and flake verification passes before MR
<!-- AC:END -->
