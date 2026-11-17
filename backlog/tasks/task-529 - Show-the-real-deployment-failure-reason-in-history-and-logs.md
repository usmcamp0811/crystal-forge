---
id: TASK-529
title: Show the real deployment failure reason in history and logs
status: In Progress
assignee:
  - automated-agent
created_date: '2026-11-17 14:00'
labels:
  - agent
  - server
  - web-ui
  - deployments
dependencies: []
priority: high
type: bug
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owner report (chesty, 2026-11-17): every failed deploy shows "Deploy failed to activate" in the History tab and in the Logs tab. The Logs tab also shows made-up lines "deploy: evaluating configuration", "activation failed", and "deploy: rolled back to previous generation". The agent journal shows that the deploy failed in the cache copy step, before activation, and no rollback occurred.

The agent already reports a failure message (`POST /agent/deployment-failed`). The server stores it in `pending_system_deployments.failure_message` and in the `cf_deployment_failed` event metadata (`failure_message`). The History API and the agent-events log query ignore it and return a fixed title.

The stored message itself is not useful for chesty: `Authenticated cache copy failed (Nix failed for an unrecognized reason; details are redacted)`. `classify_read_failure` knows only five categories. The owner wants more fixed categories. Raw Nix stderr must not leave the agent because it can contain cache URLs and credential paths.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `classify_read_failure` recognizes more common Nix copy failures with fixed, credential-free categories: DNS resolution, connection refused or unreachable, network timeout, interrupted transfer, cache server error (HTTP 5xx), rate limiting (HTTP 429), hash mismatch, local disk full, and Nix daemon or store permission problems. Raw stderr is still never logged or reported.
- [ ] #7 The agent appends the last three Nix `error:` lines to the failure message. It masks every value it gave Nix for the read (cache URLs, Basic credentials, credential paths and their directories) with `****`, removes URL user information, and masks every URL query string. Other lines, including HTTP response bodies, are dropped. (Owner decision, 2026-11-17.)
- [ ] #2 The History API title for a `cf_deployment_failed` event includes the stored failure message when it exists.
- [ ] #3 The agent-events log line for a `cf_deployment_failed` event uses level `error` and includes the stored failure message when it exists.
- [ ] #4 The Logs tab shows one reconstructed line for a failed deploy with the real failure reason. It no longer shows the invented evaluation, activation, and rollback lines.
- [ ] #5 Unit tests cover the new categories and the failure title.
- [ ] #6 `docs/knowledge/caches/niks3-cache.md` lists the new categories.
<!-- AC:END -->

## Non-goals

- Sending unmasked Nix stderr or non-`error:` lines to the server.
- Changing retry, timeout, or cache selection behavior.
- Changing the non-Niks3 copy path (TASK-507).
