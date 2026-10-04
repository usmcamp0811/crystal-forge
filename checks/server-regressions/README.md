# Server Regressions Check

This check compiles and runs a curated set of `cf-server` Cargo integration
and library tests against a disposable PostgreSQL instance, using
`postgresqlTestHook`. It is a Nix `buildRustPackage` derivation whose
`checkPhase` is the entire point of the build; nothing under `installPhase`
is meant to be consumed as a package.

## What it verifies

- A populated pre-TASK-433 (migration `0232` and earlier) database upgrades
  cleanly through the full migration set, and every populated row family
  (systems, policies, CVE scans, desired targets, compliance bundle
  assignments and versions, system states, attention occurrences, and
  notifications) survives the upgrade with the expected post-migration
  columns and indexes present. This is an upgrade rehearsal, not just a
  from-empty migration run.
- A focused list of `cf-server` Cargo integration test binaries: assignment
  semantics, composite policy, evidence-for-ATO, framework version ID
  lifecycle, policy counts, policy editor phase 2, POA&M workflows, TASK-433
  assignment visibility and CSRF, and time-window policy behavior.
- A curated list of `#[ignore]`d library tests covering POA&M
  authorization/CSRF, setup-wizard progress counting, policy-requirement
  identity hydration, notification queries and email tasks, POA&M overdue
  attention reconciliation, composite AC3 pure validation and
  JSON/TOML/CF-native interchange, the composite AC3 Nix-executor matrix,
  resolver enforcement, immutable deletion lifecycle, trusted STIG
  mapping-pair rules, bundle requirement baseline lifecycle, bundle summary
  query-count bounds, and system agent key rotation authorization/audit
  behavior.
- TASK-470 cache scope rollback preserves encrypted credentials and environment
  assignments atomically. Signed agent capabilities, assigned-first selection,
  preclaim capability fencing, dispatch identity, canonical environment
  ambiguity, and durable publication queue provenance use explicitly qualified
  library test names. Each TASK-470 invocation uses `--exact` and requires one
  successful test with zero ignored tests; a missing or renamed test fails the
  check. Only the selected database tests receive `--ignored`.
- Exact completed publication evidence binds deployment reads to the authorized
  derivation and output. The selected PostgreSQL regressions cover renamed and
  secondary sources, capability/private-read gates, current environment scope,
  deleted identities, legacy/global evidence, ambiguity, retained/archive
  delivery, retryable history and bridge requests, and configuration/assignment/
  deletion races before claim locks. Assigned gates cannot downgrade to an
  evidenced global source.

## Why it is a separate check

`packages/default/default.nix` builds and tests `cf-server` with
`--lib --bins` only; Cargo integration targets under
`crates/cf-server/tests/` are not compiled by `nix build .#server`, and the
NixOS `integration` check runs the Python `cf_test` suite, not these Rust
targets. This check exists to run PostgreSQL-backed Rust regressions that
would otherwise never execute anywhere. It is intentionally a curated list,
not `--all-targets --ignored`: the repository also contains manual, slow, and
environment-specific ignored tests that do not belong in this gate.

## TASK-470 selected library tests

The following exact names run with `--ignored --exact --test-threads=1`:

```text
queries::cache_destinations::atomic_scope_tests::create_scope_failure_leaves_no_cache_credentials_or_global_fallback
queries::cache_destinations::atomic_scope_tests::update_scope_failure_preserves_entire_config_ciphertext_and_assignments
queries::cache_destinations::tests::niks3_selection_assigned_first_disabled_fallback_and_stable_order
queries::cache_destinations::tests::niks3_assignment_writers_wait_for_publication_snapshot
handlers::agent::heartbeat::tests::niks3_signed_handler_preserves_pending_deployment_and_delivers_only_selected_reads
handlers::api::builders::tests::niks3_builder_and_agent_selection_share_assigned_first_policy
queries::builders::tests::niks3_canonical_environment_dispatch_rejects_ambiguity_before_credentials
queries::builders::tests::niks3_canonical_environment_completion_rejects_post_dispatch_ambiguity
queries::builders::tests::niks3_preclaim_handler_rejects_legacy_and_allows_capable_builder
queries::builders::tests::niks3_exact_candidate_claim_never_substitutes_after_queue_races
queries::builders::tests::niks3_dispatch_identity_and_completion_transaction_rechecks
queries::builders::tests::niks3_missing_push_queues_exact_id_and_requires_authoritative_output
queries::cache_push::niks3_tests::niks3_recorded_no_cache_dispatch_differs_from_unrecorded_enqueue
queries::cache_push::niks3_tests::niks3_queue_persists_provenance_pins_legacy_and_retains_deleted_identity
queries::cache_publication_reads::tests::niks3_publication_exact_identity_rename_and_secondary_source
queries::cache_publication_reads::tests::niks3_publication_private_capability_and_current_scope_retry
queries::cache_publication_reads::tests::niks3_publication_deleted_id_legacy_and_global_evidence
queries::cache_publication_reads::tests::niks3_publication_database_precedes_legacy_and_ambiguity_fails_closed
queries::cache_publication_reads::tests::niks3_publication_manual_pinned_auto_latest_retained_archive
queries::cache_publication_reads::tests::niks3_publication_unknown_historical_and_bridge_remain_retryable
queries::cache_publication_reads::tests::niks3_publication_rotation_race_before_claim_locks
queries::cache_publication_reads::tests::niks3_publication_assignment_race_before_claim_locks
queries::cache_publication_reads::tests::niks3_publication_deletion_race_before_claim_locks
queries::cache_publication_reads::tests::niks3_publication_assigned_gates_never_downgrade_to_proven_global
```

These non-ignored tests run with `--exact --test-threads=1`:

```text
handlers::agent_request::tests::niks3_capability_requires_authenticated_body_and_ignores_unsigned_headers
handlers::agent::heartbeat::tests::niks3_selected_cache_never_drops_private_or_unsupported_first_for_fallback
handlers::api::builders::tests::niks3_preclaim_capability_gate_preserves_legacy_cache_dispatch
```

## Run it

```sh
nix build .#checks.x86_64-linux.server-regressions --print-build-logs
```

No VM is booted. `postgresqlTestHook` starts a local disposable PostgreSQL
instance for the build sandbox; the test role has `LOGIN SUPERUSER CREATEDB`
so migration and immutability-trigger tests can exercise the same DDL
privilege level as production migrations. `SQLX_OFFLINE=true` is set, so
SQLx's compile-time query verification uses committed offline metadata
rather than a live database connection during compilation.
The check supplies `CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY` with a public,
sandbox-only fixture value. SQLx tests create their private databases through
the sandbox role; no host database or deployment credential is required.

## Out of scope

- General server/database/dashboard behavior belongs to the `integration`
  check; this check is deliberately narrow and additive to it.
- This is not a general "run every ignored test" gate. Extending the list
  requires judgment about whether a test is a genuine data-integrity
  regression versus a slow or environment-specific manual test.

## CI

Part of the `.gitlab-ci.yml` `flake-check` matrix
(`CHECK_NAME: server-regressions`), so it runs on every merge request and on
`main`.
The generated job is `flake-check: [server-regressions]`, uses the existing
`nix` runner tag, and is blocking. Membership does not establish a pass for the
exact commit under review.
