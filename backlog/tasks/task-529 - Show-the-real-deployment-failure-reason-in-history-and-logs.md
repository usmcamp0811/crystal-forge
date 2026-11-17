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
- [ ] #7 For complete stderr without a response-body delimiter, the agent appends at most the last three masked `error:` lines to the failure message. It masks every value it gave Nix for the read (cache URLs, Basic credentials, credential paths and their directories) with `****`, removes URL user information, and masks every URL query string. If a response-body delimiter appears or stderr capture is truncated/incomplete, it suppresses all detail. (Owner decision and second-review security requirement, 2026-11-17.)
- [ ] #2 The History API title for a `cf_deployment_failed` event includes explicitly safe stored display text when it exists. Legacy raw metadata is not displayed or returned; otherwise the title is `Deployment failed`.
- [ ] #3 The agent-events log line for a `cf_deployment_failed` event uses level `error` and includes only explicitly safe stored display text when it exists.
- [ ] #4 The Logs tab shows one reconstructed line for a failed deploy with the real failure reason. It no longer shows the invented evaluation, activation, and rollback lines.
- [x] #5 Unit tests cover the new categories and the failure title.
- [ ] #6 `docs/knowledge/caches/niks3-cache.md` lists the new categories.
<!-- AC:END -->

## Non-goals

- Sending unmasked Nix stderr or non-`error:` lines to the server.
- Changing retry, timeout, or cache selection behavior.
- Changing the non-Niks3 copy path (TASK-507).

## Integration test impact (review P2)

- Authority: the MR !344 review requires a safe display boundary for legacy and
  non-Niks3 failures. The implementation adds optional agent `display_error`,
  persisted `metadata.display_failure_message`, and neutral
  `Deployment failed` history/log fallback. Raw `metadata.failure_message` is
  never display-safe. New pending rows contain the safe diagnostic or neutral text.
- Consumers: deployment-failed authentication/persistence, History API,
  agent-events API, and Systems History/Logs. Extend the existing Delivery
  `delivery-lifecycle` with a separate unbooted probe registration. Retain all
  existing activation, membership, rollback and management assertions.
- Disposition: add signed report, unsigned/no-mutation, safe long diagnostic,
  legacy report, blank safe report, and historical missing/blank/raw-only metadata
  cases. Historical rows are compatibility fixtures, not proof of agent reporting.
  Browser assertions protect failed state, error severity, one reconstructed line
  per failure, absence of invented evaluation/activation/rollback, reload and overflow.
- The signed fixture proves reporting and display boundaries only. The separate
  agent failure-wrapper test owns actual Nix diagnostic redaction proof.
- Verification: focused farm gate:
  `nix build .#checks.x86_64-linux.integration-delivery.scenarios.delivery-lifecycle -L`.
  JavaScript syntax, Python AST parsing, Nix parsing and `git diff --check`
  passed locally using existing Nix-store tools. The new API/browser cases have
  not executed; no browser or screenshot evidence is claimed.

## MR !344 review remediation

- P1 short secrets: mask all nonempty values without trimming or a minimum
  length. Tests cover one-, two-, and three-character credentials, including an
  authenticated-copy child failure and safe report serialization.
- P1 display boundary: only typed authenticated-copy errors supply `display_error`;
  outer contexts and other paths supply neutral text. Ingress does not persist
  legacy raw `error`. History, agent-events, and progress read only explicit safe
  event metadata; History also strips legacy raw metadata from its response.
  TASK-507 still owns non-Niks3 local journal behavior.
- P2 classification: recognize every status from 500 through 599, tested
  exhaustively; reject 499, 600, and 5000 as server-error categories.
- P2 fallback: use `Deployment failed` for History without safe diagnostics.
- P2 regression: extend the existing Delivery scenario; do not claim the authored
  signed report proves agent-side redaction. The agent child-process tests prove
  masking and selection for display separately.
- Farm component verification passed for `default.cf-agent-drv` and
  `default.cf-server-core-drv`. The Delivery gate did not reach runtime: initial
  attempts failed building VM dependencies, then the embedded server test
  `handlers::api::builders::tests::niks3_probe_failure_never_polls_success_transition`
  failed (`Process` versus `AuthenticationFailed`). That code is unchanged here.
- Preview is blocked: the supported Dioxus launchers compile locally, prohibited
  by the build-farm policy. The embedded server farm build failed as above. No
  browser screenshot, overflow result, or green pipeline is claimed. Keep this
  task In Progress pending successful Delivery/browser verification.
- MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/344

## Second review: provider-controlled response bodies

- Suppress the entire masked detail when stderr contains `response body:`
  anywhere, case-insensitively. Do not infer that a later `error:` line marks
  the end of a body. The fixed category and exit status remain available.
- The bounded collector now records whether it retained the entire stream.
  Truncation, read failure, drain timeout, or join failure suppress detail;
  a retained tail could have lost the response-body delimiter.
- Tests cover a body beginning with `error: internal-token=secret-value-12345`,
  mixed-case and inline delimiters, and a large body that evicts its delimiter.
  Fake-Nix tests exercise authenticated-copy failures, typed safe-display
  selection, report serialization, and credential cleanup. The older oversized
  signature test now expects a fixed category without any captured detail.
- Verification on the configured farm:
  - `nix build .#default.cf-agent-drv .#okf-knowledge --no-link -L`: passed;
    agent test groups passed 25, 13, and 1 tests.
  - An ephemeral `cf-server-drv.overrideAttrs` narrowed only `checkPhase` to
    `cargo test --offline --locked --release --package cf-server --features
    cf-server/embedded-ui --lib
    handlers::api::builders::tests::niks3_probe_failure_never_polls_success_transition
    -- --exact --nocapture`: passed independently (1 test) on the same farm.
  - `nix build .#default.cf-server-drv --no-link -L`: passed normally, including
    the full library group (1575 passed, 853 ignored under existing policy).
    No production package test was skipped or changed to obtain this result.
  - `nix build .#checks.x86_64-linux.integration-delivery.scenarios.delivery-lifecycle
    --no-link -L`: ran the normal VM scenario, but hit its 1800-second timeout
    in `publication-rejects-untrusted-signature-then-accepts-same-signed-output`.
    The driver was waiting for the existing `services` node's `nix copy --from
    daemon --to ...` command. The final bad-file-descriptor exception followed
    VM shutdown. The new failure-report/browser cases were not reached.
- Keep the MR Draft and task In Progress. Delivery browser behavior, overflow,
  screenshots, and exact-head CI remain unverified. The embedded-server test
  is no longer the current blocker; the full Delivery timeout is.
