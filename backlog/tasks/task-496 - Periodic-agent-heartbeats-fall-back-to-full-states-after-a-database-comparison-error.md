---
id: TASK-496
title: >-
  Periodic agent heartbeats fall back to full states after a database comparison
  error
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-19 05:37'
updated_date: '2026-10-20 05:23'
labels: []
dependencies: []
references:
  - packages/default/crates/cf-server/src/handlers/agent/heartbeat.rs
  - packages/default/crates/cf-server/src/queries/system_states.rs
  - checks/integration-delivery/scenarios/delivery-lifecycle.py
  - /tmp/opencode/delivery-local-run6.log
priority: medium
type: bug
ordinal: 547000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The real target agent registers and sends signed periodic heartbeat requests, but the server repeatedly logs 'Heartbeat became state change: Database error occurred while checking state' and persists a new full state. Observed in the five-role Delivery lifecycle based on e07b4bd2. The agent journal records periodic heartbeat sends; the controller accepts each request. Registration and periodic reporting work, but lightweight heartbeat classification is not proven correct. This product defect is outside the no-production-changes test overhaul.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 An unchanged registered target's periodic signed heartbeat is classified without a database comparison error.
- [x] #2 Persisted heartbeat and state identities remain correlated to the exact registered target and running generation.
- [x] #3 A genuine state change still creates the required full-state evidence rather than being misclassified as an unchanged heartbeat.
- [x] #4 An integration or isolated-database regression and affected source documentation describe and verify the intended classification and failure behavior.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Owner-authorized narrow product repair, based on f6439851 in the dedicated TASK-496-heartbeat-classification worktree. Coordinator openai owns integration and real-agent scenario edits; bounded worker owns only active heartbeat/query Rust code and its regressions.
1. Reproduce the actual latest-state decoder failure with the migrated schema. SystemState expects boot_id while SELECT * from system_states has no such column; verify this before choosing a projection fix. Preserve systems.boot_id as reboot baseline, historical state semantics and conservative real-error behavior.
2. Apply the smallest justified query/result-shape correction, without schema/protocol redesign or unrelated generation policy changes. Add an ignored migrated query regression and signed-handler sequence proving initial state, repeated unchanged heartbeats, a genuine compared state change and later heartbeat correlation; include exact registered IDs and a foreign-host control.
3. Run the affected Rust/DB tests against an invocation-owned isolated database only, with offline Cargo/SQLx and explicit preparation. Worker must not start competing VM stacks or use the user's development database. Coordinator adds exact selections to the existing consolidated runner.
4. Commit the product fix separately after review and integrate it into the coordinator branch. Replace Delivery's permissive heartbeat/full-state fallback proof with exact real-agent assertions, then exercise changed running generations in the same activation/rollback lifecycle. Record real artifacts and hosted results; no merge/deploy/Done.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
The owner explicitly authorized this smallest production repair and regressions in the current assignment. Dedicated worktree/branch TASK-496-heartbeat-classification is based on the refreshed coordinator reporting head f6439851. No schema redesign, deployment-policy/protocol change or unrelated agent repair is authorized.

Separately attributable product repair: original5c8f5ee8 in TASK-496-heartbeat-classification; coordinator application d11de05967b2ade5c7e1f833317039de134c307a. It corrects historical SystemState row decoding with NULL::text AS boot_id; no schema/protocol redesign. systems.boot_id remains the reboot baseline. Commit3db47c8953e3cf1f29473be7595a660d69e6a988 adds consolidated-runner selections and strict unchanged-heartbeat integration assertions and is pushed to draft MR335. Hosted pipeline2965867360 passed all8jobs, exact clean3db47c89 artifacts Access23/Delivery10/DB517pass0fail0ignored229summaries. Artifacts downloaded and inspected; this pipeline proves the earlier unchanged subset, not later activation code.

Changed-state acceptance now has real-agent evidence, not only a fabricated signed-handler payload: normal Delivery CI command executed in 3db47c89-dirty connected worktree on x86_64-linux. Initial running state1 has heartbeats1,2. Production UI deployed authored commit e5c115237b3d026f65dbfe74a86331e820a36c78; actual running/profile generation1 output /nix/store/wii9frwfwvkrl26a80kwizd3yrgq7q0l-nixos-system-targetA-test produced exactly one new full state2 and heartbeats6,7. Second real CF-authored/evaluated/built/published/UI-deployed revision9d79b584629e73ccdee7806d04d2d46217759581 activated generation2 output /nix/store/cz8flym2r8cxn39k1xwfj6m6244cpdld-nixos-system-targetA-test; exactly one new full state3, heartbeats9,10. Registered systemf1addf9f-3f84-4b8b-9598-c244d29ae307. No direct state writes, fake report, stopped agent, duplicate unchanged full states or comparison errors.

Evidence /nix/store/mrs8rm4yvf5lk9vklqs01qvmzp1zl9n7-vm-test-run-cf-it-delivery-delivery-lifecycle-run/result.json and browser/journals. OverallDelivery remainsFAILED13passed/1failed at mandatory retained rollback eligibility (TASK499); this is independent of the now-executed changed-state classification. Keep task In Progress pending integrated publication/hosted handoff; do not mark Done. Rollback acceptance and parent overhaul are not satisfied by this task.
<!-- SECTION:NOTES:END -->
