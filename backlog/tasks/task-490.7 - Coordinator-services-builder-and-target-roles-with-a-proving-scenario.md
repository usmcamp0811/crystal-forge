---
id: TASK-490.7
title: 'Coordinator: services, builder, and target roles with a proving scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:36'
labels:
  - testing
  - integration
  - coordinator
dependencies:
  - TASK-490.1
parent_task_id: TASK-490
priority: high
type: task
ordinal: 519000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Coordinator-owned (not a Luna packet): writes tests/integration/lib/** only, plus one proving scenario under tests/integration/delivery/scenarios/.

WHY: Delivery, Assurance, OIDC, and notification packets need real services, builder, and target roles. Packets must not depend on planned interfaces, so each role is added only with an executed scenario that proves it.

SCOPE: add to tests/integration/lib/roles.nix: services { git ? false, oidc ? false, tls ? false, caches ? [], notificationReceiver ? false } (local Git HTTP fixture derived from lib/test-flake, local OIDC provider, test CA and TLS origin for cross-host browsers, cache fixtures reusing lib/cache-test-node, receiver); builder { name, strategy } for builderA/builderB using services.crystal-forge.build with API mode and its own credentials; targetA as a bootable NixOS target with the real agent (services.crystal-forge.client) and its own key. Provide helpers: fresh-output fixture with per-run nonce (for assert_store_absent), agent/builder key provisioning, and an offline flake registry from lib.crystal-forge.prefetchedPaths. Prove with delivery/scenarios/evaluation-build-smoke (real commit -> evaluation -> real build on builderA). Update the coverage map harness section with the new signatures. Non-goals: the Delivery obligations themselves (490.8-490.15).

Source to read: lib/gitserver-test-node, lib/cache-test-node, lib/agent-test-node, lib/test-flake, checks/niks3-cache/default.nix, checks/web-ui/default.nix (gitserver/cache wiring), modules/nixos/crystal-forge (build.*, client.*, cache.encryption_key_file), docs/knowledge/builders/*.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 services, builderA/builderB, and targetA role functions exist with documented arguments and are used by an executed scenario
- [ ] #2 The proving scenario evaluates a real commit and builds a fresh output on builderA that was absent beforehand
- [ ] #3 The coverage map harness interface documents the new role and helper signatures
- [ ] #4 No production code or module semantics change
<!-- AC:END -->
