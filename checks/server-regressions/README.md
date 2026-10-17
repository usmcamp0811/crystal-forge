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
  semantics, compliance assignment zombie repair, composite policy,
  evidence-for-ATO, framework version ID lifecycle, policy counts, policy editor
  phase 2, POA&M workflows, TASK-433
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
- Signed builder next-job requests use a real loopback TCP listener and Axum
  `ConnectInfo`. Credentialed Attic, S3, and Niks3 dispatch requires the trust
  flag, an allowed actual direct peer, and one exact `https` protocol header.
  The matrix rejects missing peer/header, duplicate headers, protocol chains,
  HTTP, and case variants. Public Http/Nix dispatch still succeeds in each
  negative proxy case. Successful claims record the exact builder session and
  cache identity. Anonymous requests and unsigned capability headers cannot
  authorize claims. TLS termination is exercised by the separate Niks3 VM check.
- Exact completed publication evidence binds deployment reads to the authorized
  derivation and output. The selected PostgreSQL regressions cover renamed and
  secondary sources, capability/private-read gates, current environment scope,
  deleted identities, legacy/global evidence, ambiguity, retained/archive
  delivery, retryable history and bridge requests, and configuration/assignment/
  deletion races before claim locks. Assigned gates cannot downgrade to an
  evidenced global source.
- Niks3 CA loading projects recognized legacy bundles to ordinary certificate
  roots in memory. Create and CA-only Save persist canonical roots without
  changing retained private-key ciphertext. GET, Test, and Cancel preserve the
  raw row and assignments. Client chains remain certificate-only, and a
  mismatched write identity fails before DNS. Static field-scoped diagnostics
  never include submitted material or upstream response text. Custom write-root
  policy skips built-in and ambient CA files; the native Niks3 VM check remains
  the authoritative end-to-end TLS and publication proof.

## Why it is a separate check

`packages/default/default.nix` builds and tests `cf-server` with
`--lib --bins` only; Cargo integration targets under
`crates/cf-server/tests/` are not compiled by `nix build .#server`, and the
NixOS `integration` check runs the Python `cf_test` suite, not these Rust
targets. This check exists to run PostgreSQL-backed Rust regressions that
would otherwise never execute anywhere. It is intentionally a curated list,
not `--all-targets --ignored`: the repository also contains manual, slow, and
environment-specific ignored tests that do not belong in this gate.

## TASK-484 configuration-discovery database tests

These six ignored, migration-backed SQLx tests run against sandbox-owned
PostgreSQL databases. Each runs in a separate Cargo process because the
heavy-Nix semaphore is process-global. The runner uses
`--ignored --exact --test-threads=1` and requires exactly one successful test
with zero ignored tests. A missing or renamed test fails the exact-test guard.

```text
models::evaluate_with_policies::tests::discovery_database_tests::discovery_advisory_precedes_semaphore_and_releases_before_bulk
models::evaluate_with_policies::tests::discovery_database_tests::discovery_quarantine_retains_real_advisory_and_capacity
models::evaluate_with_policies::tests::discovery_database_tests::discovery_failures_and_cancellation_terminalize_exact_attempt
models::evaluate_with_policies::tests::discovery_database_tests::discovery_api_cancellation_stops_active_child_and_finalizes_original_attempt
models::evaluate_with_policies::tests::discovery_database_tests::discovery_api_cancellation_interrupts_advisory_and_capacity_admission
models::evaluate_with_policies::tests::discovery_database_tests::bulk_admission_preserves_owned_leases_and_bounds_pool_wait
```

- **Admission order and release:** A real PostgreSQL advisory-lock blocker keeps
  discovery waiting without consuming semaphore capacity. After the advisory
  lock becomes available, a separately held permit still prevents the operation
  from starting. The operation observes both locks owned. Completion releases
  both locks and a second admission succeeds, proving the preflight scope does
  not retain a nested lease before bulk admission.
- **Quarantine ownership:** A fixture-owned process group remains alive after
  cleanup reports unconfirmed. The fixture simulates a signal that has not
  become effective; it does not create a real kernel `D`-state process. The real
  advisory lock and semaphore capacity remain unavailable beyond the preflight
  deadline and cleanup acknowledgement allowance. Only an explicit fixture
  kill, direct-child reap, and confirmed group absence allow both locks to
  become available again.
- **Exact-attempt terminal state:** Bounded process timeout and a deterministic
  syntax-error diagnostic pass through production failure classification and
  persistence. With one transient retry enabled, timeout marks the exact attempt
  failed with `completed_at` and schedules a retry, leaving the commit pending.
  The syntax error permanently fails the commit and exact attempt. Requested
  cancellation passes through the guarded cancellation finalizer and leaves
  both commit and exact attempt cancelled with the attempt completion recorded.
  None of these cases leaves that attempt in progress.
- **Requested cancellation:** A real database cancellation request stops an active
  discovery child and finalizes the original attempt. Requests also interrupt
  advisory-lock and semaphore waiting during discovery and bulk admission.
  Runtime cancellation queries have a 500-millisecond ceiling and a two-second
  interval between polls. Discovery grants at most two seconds for foreground
  cleanup after a known request; unconfirmed cleanup keeps lock ownership in
  quarantine. Childless admission drops immediately on a known request.
- **Bulk admission:** Successful admission returns both owned leases. Pool
  acquisition cannot wait beyond its admission deadline. The production
  `eval_overall_timeout_secs` admission ceiling covers pool, advisory-lock, and
  semaphore waiting plus the final cancellation check. Main execution receives
  its established overall budget only after admission and resource planning;
  admission time is not subtracted from that budget.

These tests exercise the Rust coordinator, process ownership, and database
finalizers with controlled process fixtures. They do not measure private-input
downloads or run real Nix discovery. The separate
[cold/warm discovery check](../nixos-discovery-reliability/README.md) proves
the exact-source, lock-integrity, private-input, and discovery-to-policy path
with a controlled 65-second input response. Check membership describes coverage;
it does not establish a passing result for the current worktree.
The Python fixture's timeout cleanup sends `killpg(SIGKILL)` and waits for the
direct child. That fixture does not assert process-group absence or prove Rust
quarantine ownership; those are separate coordinator regression contracts.

## TASK-488 evaluation policy capture tests

These four ignored, migration-backed tests use SQLx-created sandbox databases.
The exact library-test runner invokes each qualified name with
`--ignored --exact --test-threads=1`. The guard requires its named success line
and exactly one passed test with zero failures and zero ignored tests. Missing
or renamed tests fail the check.

```text
queries::eval_policy_plans::tests::capture_is_atomic_first_write_immutable_and_retains_partial_rows
queries::eval_policy_plans::tests::capture_rejects_wrong_attempt_owner_and_retry_cannot_read_old_plan
server::policy_capture_tests::real_loader_captures_overlap_empty_contexts_and_immutable_history
server::policy_capture_tests::real_virtual_baseline_capture_preserves_bundle_without_assignment_uuids
```

The query tests cover atomic first-write capture, immutable history, partial
rows, attempt ownership and retry isolation. The loader tests exercise resolved
overlap, empty contexts and historical membership, including virtual baseline
bundles without assignment UUIDs. The separate browser-local matrix fixture
tests presentation; it does not prove persisted capture. Runner membership does
not establish a passing result for the current worktree.

### MR334 review regressions

The runner adds these 16 ignored, migration-backed SQLx tests since `3bf1c002`.
Each qualified name uses the same exact-success guard described above. The
existing 73 ignored and 83 non-ignored library selections retain their names and
relative execution order.

```text
queries::eval_policy_plans::scoped_read_tests::mixed_flake_projects_all_metadata_and_shared_configuration
queries::eval_policy_plans::scoped_read_tests::foreign_missing_and_removed_membership_are_nondisclosing
queries::eval_policy_plans::scoped_read_tests::deleted_moved_and_reconfigured_hosts_fail_closed
queries::eval_policy_plans::scoped_read_tests::authorization_and_evidence_share_repeatable_read_snapshot
queries::eval_policy_plans::scoped_read_tests::legacy_configuration_names_cannot_grant_historical_host_access
queries::eval_policy_plans::scoped_read_tests::pinned_reads_preserve_scope_and_revoke_historical_access
queries::commits::policy_counts::tests::native_ten_passes_then_queued_retry_has_zero_current_counts
queries::commits::policy_counts::tests::current_partial_native_and_mock_counts_exclude_stale_and_pending
queries::commits::policy_counts::tests::markerless_legacy_counts_are_unavailable_not_passing
handlers::api::systems::evaluation_lifecycle::tests::lifecycle_mutation_scopes_authority_audits_and_cleans_up
handlers::api::systems::evaluation_lifecycle::tests::lifecycle_handler_enforces_auth_csrf_and_nondisclosing_scope
handlers::api::systems::evaluation_lifecycle::tests::lifecycle_audit_failure_rolls_back_classification
server::policy_capture_tests::capture_display_metadata_and_classification_share_repeatable_read_snapshot
queries::eval_policy_plans::pinned_read_tests::pin_belongs_to_commit_and_never_falls_back_to_new_retry
queries::eval_policy_plans::pinned_read_tests::historical_mock_values_survive_retry_while_stale_native_is_unknown
queries::eval_policy_plans::pinned_read_tests::pinned_native_failure_is_preserved_until_its_marker_is_overwritten
```

- **Current scope:** Mixed-environment and shared-configuration reads project
  rows, columns, bundles, memberships, details, flat evidence, and mock presence
  together. Foreign or missing commits and revoked membership do not disclose
  evidence. Deleted, moved, or reconfigured hosts fail closed. Legacy configuration
  names cannot grant historical host access. Authorization and evidence use one
  read-only REPEATABLE READ snapshot; a fresh read observes revocation.
- **Current counters and native-unavailable evidence:** Queue and history use the
  represented attempt. A queued retry has zero current counts; partial current
  native and mock outcomes exclude stale and pending rows. Dual current carriers
  make availability false. Markerless or malformed legacy evidence cannot become
  passing counts. The existing guarded native-finalizer and mock-projection tests
  also retain Unknown controls and withhold stale or malformed native diagnostics.
  Unavailable native evidence is not proof of a pass or an absent assignment.
- **Operator Greenfield classification:** Lifecycle mutations require operator or
  admin authority, current environment scope, and CSRF protection. Tests cover
  explicit true, false, and null classification, actor/timestamp persistence,
  atomic audit, denied callers, moves, revocation, admin access to unscoped hosts,
  deletion cleanup, and rollback when audit insertion fails. Classification is
  operator data; names and missing deployment state do not infer Greenfield.
- **Capture metadata:** The new loader test reads classification, exact-version
  descriptions, safe source identifiers, and trusted requirement mappings from
  one REPEATABLE READ snapshot while concurrent operator/catalog edits occur.
  It excludes suggested mappings and executable or secret metadata. The existing
  loader test also preserves captured classification and display metadata after
  current catalog/classification edits.
- **Pinned contexts:** Pins belong to the requested commit and never fall back to
  a newer retry. Responses project exact commit SHA, attempt UUID/number, status,
  and start/completion timestamps without changing the stored capture. Historical
  mock outcomes survive retry. These original native fixtures exercise mutable
  fallback: overwritten markers leave old controls Unknown and hide retry
  diagnostics, while exact old failure evidence remains visible only while its
  marker survives. Immutable native retention is covered separately below.
  The scoped pin test rechecks current
  membership and withholds foreign metadata and commit-global errors.

These declarations describe registered coverage. Parent-owned targeted runs are
separate evidence; registration does not establish an exact-head CI pass.

### TASK-488 exact inventory and visibility regressions

These 24 ignored, migration-backed SQLx cases use the same exact-success guard.
Each qualified name runs with `--ignored --exact --test-threads=1`; both the
named success line and the one-passed/zero-failed/zero-ignored summary are required.

```text
queries::evaluation_inventory::tests::inventory_normalizes_exact_names_and_rejects_replacement
queries::evaluation_inventory::tests::inventory_empty_and_selection_subset_are_distinct_from_missing
queries::evaluation_inventory::tests::inventory_schema_enforces_owner_immutability_and_parent_cascade
queries::evaluation_inventory::tests::inventory_rejects_cancelled_superseded_and_nonactive_writers
queries::evaluation_inventory::tests::inventory_retry_never_inherits_or_accepts_older_writes
queries::evaluation_inventory::tests::inventory_competing_first_writes_preserve_one_complete_record
queries::commits::inventory_summary_tests::exact_inventory_replaces_cached_41_and_excludes_unselected_outcomes
queries::commits::inventory_summary_tests::inventory_partial_unknown_ambiguous_and_queued_retry_remain_pending
queries::commits::inventory_summary_tests::scoped_summaries_hide_shared_policy_facts_search_errors_and_revoked_membership
handlers::api::commits::visibility::tests::mixed_environment_flake_denies_unstructured_evidence
handlers::api::commits::visibility::tests::revocation_rechecks_each_snapshot_and_keeps_reads_atomic
handlers::api::commits::visibility::tests::unmanaged_discovery_name_denies_even_with_all_memberships
handlers::api::commits::visibility::tests::missing_inventory_and_queued_retry_never_inherit_authority
handlers::api::commits::visibility::tests::all_visible_configs_allow_logs_and_mutable_latest_graph
handlers::api::commits::visibility::tests::shared_configuration_hidden_host_denies_even_when_inactive
handlers::api::commits::visibility::tests::missing_identity_or_registration_never_grants_vacuous_access
handlers::api::commits::visibility::tests::mutable_graph_preserves_counts_but_denies_unmanaged_retained_rows
handlers::api::commits::visibility::tests::stream_revalidation_observes_membership_roles_and_session_revocation
queries::eval_policy_plans::inventory_projection_tests::inventory_discovered_41_preserves_exact_names_and_12_captured_hosts
queries::eval_policy_plans::inventory_projection_tests::inventory_absent_and_unselected_registered_rows_keep_unknown_evidence
queries::eval_policy_plans::inventory_projection_tests::inventory_shared_hosts_count_one_matching_global_failure_without_fake_policies
queries::eval_policy_plans::inventory_projection_tests::inventory_history_ignores_cache_mutation_and_queued_retry_has_no_inventory
queries::eval_policy_plans::inventory_projection_tests::inventory_scoped_reads_hide_unmanaged_hidden_columns_and_global_failure_details
queries::eval_policy_plans::inventory_projection_tests::inventory_missing_discovery_is_unavailable_despite_passing_native_and_cache
```

- **Inventory persistence (six cases):** Migration `0306` records immutable
  exact-attempt discovered and selected names with native/mock provenance.
  Cases cover exact spelling, sorting/deduplication, empty versus missing
  discovery, selection subsets, first-write conflicts, schema ownership and
  parent cleanup, cancellation, supersession, retries, and competing writers.
- **Queue/history summaries (three cases):** Exact discovery replaces mutable
  cached names. Counters exclude unselected and out-of-inventory outcomes and
  distinguish pending from unavailable coverage. Current membership scopes names,
  search, totals, errors, and shared-configuration policy facts; revocation removes
  access. Both summary readers use read-only REPEATABLE READ transactions.
- **Indivisible evidence visibility (nine cases):** Logs, WebSocket authorization,
  and mutable latest graphs require whole-flake scope. Cases cover mixed and
  shared environments, inactive hosts, unmanaged names, missing inventory,
  queued retries, moves, missing registration, and membership/role/session
  revocation. An authorized snapshot can finish; the next check observes changes.
- **Matrix inventory reconciliation (six cases):** Discovery extends captured host
  rows without invented IDs or applicability. Absent or unselected planned rows
  retain provenance and Unknown evidence. Shared hosts count one configuration
  gate; scoped reads hide unmanaged names, hidden columns, and global diagnostics.
  The primary population is the distinct union of planned and discovered names.
  Absent planned names and ambiguous evidence are unavailable. Missing results
  are pending only while the current attempt can still complete. A saved false
  intrinsic CF-agent gate can prove failure under partial shared visibility;
  hidden requirement outcomes and free-form diagnostics remain withheld.
  Cache changes cannot rewrite retained inventory, and retries inherit none.

These declarations describe registered coverage, not execution results. Native
outcome fixtures do not prove that Nix evaluation ran. Runtime and CI verification
remain separate evidence.

### TASK-488 immutable native outcome regressions

These six ignored, migration-backed SQLx cases run through the same
`--ignored --exact --test-threads=1` guard. Each invocation must report the named
success line and exactly one passed test with zero failures and zero ignored tests.

```text
queries::evaluation_native_outcomes::tests::native_outcome_rejects_cancelled_superseded_and_nonactive_owners
queries::evaluation_native_outcomes::tests::native_outcome_requires_exact_selected_native_inventory
queries::evaluation_native_outcomes::tests::native_outcome_first_write_schema_immutability_and_parent_cascade
queries::evaluation_native_outcomes::tests::native_outcome_sidecar_has_no_artifact_build_or_observation_side_effects
queries::evaluation_native_outcomes::tests::native_outcome_retry_retains_historical_values_under_new_ea
queries::evaluation_native_outcomes::tests::native_producers_atomically_retain_success_and_confirmed_failure_across_retry
```

Migration `0307` retains exact-attempt native evidence independently of mutable
derivations. The cases cover active ownership, cancellation, supersession,
exact selected native inventory, first-write conflicts, schema immutability,
parent cleanup, and absence of artifact, build, observation, and snapshot writes
by the sidecar. Production persistence/finalization cases cover atomic success
and confirmed failure, rollback, and historical values across retry.

The same cases replay the data statement from migration `0308`. Retention
requires exact discovered-and-selected native names and positive integer attempt
markers. Mock, malformed, unselected, and unattributable carriers are excluded.
Tests preserve existing immutable records and historical values after mutable
carrier overwrite, and verify explicit unavailable records and native/mock
ambiguity. The derivation uniqueness constraint prevents seeding duplicate native
carriers; the schema case checks unavailable-state restrictions directly.
Registration does not establish execution or a passing current-source result.

### System and Compliance receiver scope

Six migration-backed receiver tests cover single-system and bundle-system reads,
scope revocation, hidden rollups, and preservation of empty/legacy responses.
Two cases use PostgreSQL lock wait inspection to prove that membership deletion
and environment moves wait until a receiver finishes materializing its response.
The bundle reader restricts candidates to the exact locked system UUID set.

### TASK-488 log, recovery, receiver, and inventory review regressions

These 20 ignored and nine pure tests preserve all 208 previous exact selections.
Each uses the existing named-success and exactly-one-passed guard. Registration
describes coverage; it does not establish a runtime or CI pass.

- **Attempt logs:** Historical rows remain stored; stale single and batch writes
  fail. Unattributed legacy logs are excluded from current-attempt reads.
  WebSocket retry replaces a hidden host UUID with a visible UUID under the same
  configuration name without replaying old-attempt evidence. Six pure tests
  cover generation fencing, replay/live ordering, bounded buffers, cleanup, all
  message variants, and compatible wire shapes.
- **Receivers:** Two additional tests materialize composite evidence with a
  one-connection pool and preserve environment scope and admin compatibility.
  Another test proves that evidence reads acquire the POA&M sentinel before
  system scope locks, matching concurrent metadata writers.
- **Restart recovery:** Seven tests retain partial durable evidence, capture a
  fresh child plan, preserve zero and exhausted failure budgets, and retain
  queued children and delayed backoff. Repeated/concurrent recovery targets the
  exact source; child insertion failure rolls back source and commit changes.
  Cancellation, terminal attempts, and missing owners never enqueue work.
  Recovery does not consume the ordinary failure retry budget or impose a hard
  cap on restart recovery. Ordinary failures still obey the configured budget.
- **Required inventory:** Four database tests check cancellation, supersession,
  timeout, connection reclamation, exact ownership, and conflicts. Three pure
  helper tests block producer admission after persistence or final-check failure.
  A fifth database test proves that PostgreSQL's 31-second local timeouts let
  an abandoned writer return its connection while the external lock stays held.
  A sixth preserves stricter database/session timeout policies.
  The actual worker test verifies permit release before blocked finalization,
  including finalizer failure. Database timeout cases use the actual production
  30-second deadline, with no shorter runner timeout or deadline override.

The worker requires a runner-provided public TOML fixture containing only:

```toml
[server]
host = "127.0.0.1"
port = 0
execution_mode = "mock"
```

The check creates this fixture with `pkgs.writeText` and scopes
`CRYSTAL_FORGE_CONFIG` only to the exact invocation of
`server::required_inventory_worker_tests::required_inventory_actual_worker_releases_permit_before_blocked_finalization`.
`process_pending_commits` reloads `CrystalForgeConfig` independently of `CFState`,
so mock mode on `CFState` alone is insufficient. No global mock override affects
integration targets or other tests.

## TASK-470 selected library tests

The exact library-test runner selects 145 ignored tests: the 62 tests in the
TASK-470 extra library list below (including four TASK-488 isolated mock tests
and four derivation-count tests, plus two native-finalizer tests),
the six TASK-484 tests, four original TASK-488 capture tests, 16 MR334 review
tests above, the 24 inventory and visibility tests above, six immutable native
outcome tests, six receiver scope tests, 20 additional TASK-488 review tests,
and this TASK-487
regression. With 92 non-ignored selections (83 existing plus nine TASK-488 pure
tests), the exact library-test runner selects 237 tests in total:

```text
handlers::api::deployment_policies::tests::test_list_deployment_policies_includes_fixture
```

The TASK-487 test counts migration-seeded policies before it creates a fixture.
The list response must contain the fixture UUID exactly once, preserve its name,
type, and enabled flag, and report the baseline count plus one.

The runner also compiles `register_export_selection` and executes these seven
ignored integration tests individually with `--ignored --exact --test-threads=1`:

```text
pages_whole_filtered_scope_and_rechecks_reader
acceptance_only_does_not_apply_unrelated_poam_filters
policy_export_context_uses_actual_waiver_finding
exceeds_combined_cap_without_partial_selection
plan_context_retains_all_linked_systems_in_one_snapshot
retired_link_in_same_environment_exports_but_moved_out_link_does_not
scheduled_environment_keeps_both_exact_packages
```

Each invocation requires its named success line and exactly one passed test with
zero failures and zero ignored tests. These tests use SQLx-created sandbox
databases. They cover complete pagination, rechecked reader authorization,
acceptance-only scope, exact finding context, combined-cap rejection, linked
system visibility, retained history, and exact scheduled package identities.
Membership does not establish a passing result for the current source.

The following 62 ignored PostgreSQL tests run with
`--ignored --exact --test-threads=1`. Each invocation must report exactly one
passed test and zero ignored tests. The tests include completed CVE publication
provenance and malformed signing-key rejection across all Niks3 auth modes.

```text
models::evaluate_with_policies::mock_persistence::tests::mock_persistence_records_attempt_outcomes_without_production_work
models::evaluate_with_policies::mock_persistence::tests::mock_persistence_cancelled_or_superseded_attempts_write_nothing
models::evaluate_with_policies::mock_persistence::tests::isolated_mock_counts_match_queue_history_and_exclude_queued_retry
queries::eval_policy_plans::tests::isolated_mock_projection_matches_attempt_and_rejects_ambiguous_native
queries::eval_policy_plans::tests::confirmed_native_finalizer_failure_is_attempt_owned_and_preserves_build
queries::eval_policy_plans::tests::confirmed_native_finalizer_cancellation_writes_no_failure_evidence
queries::commits::tests::derivation_count_isolated_current_mock_has_zero_records
queries::commits::tests::derivation_count_excludes_stale_native_and_queued_retry
queries::commits::tests::derivation_count_counts_current_native_records_not_groups
queries::commits::tests::derivation_count_markerless_legacy_is_unavailable
queries::commits::partial_resource_failure_tests::partial_resource_failure_retains_rejected_completion_with_preserved_build_history
queries::commits::partial_resource_failure_tests::partial_resource_failure_reconciles_committed_but_unacknowledged_completion
queries::commits::partial_resource_failure_tests::partial_resource_failure_ignores_never_committed_completion_candidate
queries::commits::partial_resource_failure_tests::partial_resource_failure_mixed_confirmed_and_resource_cohorts_terminalize
queries::build_jobs::tests::resource_terminal_pending_preparation_is_exact_guarded_and_activates_once
queries::commits::partial_resource_failure_tests::partial_resource_failure_retains_completed_evidence_snapshots_and_jobs
queries::commits::partial_resource_failure_tests::partial_resource_failure_rejects_unverified_retention_atomically
queries::commits::partial_resource_failure_tests::partial_resource_failure_respects_supersession_and_cancellation
handlers::api::caches::basic_read_tests::niks3_basic_get_save_test_cancel_modes_and_authority_preserve_secrets
queries::cache_publication_reads::tests::niks3_basic_publication_withholds_old_capabilities_and_insecure_delivery
handlers::api::caches::discovery_tests::niks3_stored_discovery_retains_replaces_clears_modes_without_persistence
handlers::api::caches::discovery_tests::niks3_stored_discovery_auth_invalid_json_type_tls_and_csrf_fail_without_mutation
handlers::api::caches::niks3_ca_tests::canonical_ca_get_test_cancel_and_ca_only_save_preserve_ciphertext
handlers::api::caches::retained_probe_tests::attic_named_cache_retained_replacement_results_and_policy_preserve_raw_state
handlers::api::caches::retained_probe_tests::legacy_attic_plaintext_and_historical_ciphertext_retain_on_test_and_save
handlers::api::caches::retained_probe_tests::legacy_attic_null_and_empty_token_refuse_before_probe_without_mutation
handlers::api::caches::retained_probe_tests::stored_probe_preserves_ciphertext_scope_and_timestamps
handlers::api::caches::retained_probe_tests::stored_probe_admin_json_missing_id_conversion_and_ssrf
handlers::api::caches::retained_probe_tests::add_probe_validates_without_writes_and_niks3_requires_token
handlers::api::caches::retained_probe_tests::legacy_basic_probe_preserves_raw_url_and_sanitized_roundtrip
handlers::api::caches::retained_probe_tests::legacy_query_probe_refuses_without_mutation_or_replay
handlers::api::caches::retained_probe_tests::legacy_s3_presigned_endpoint_redacts_and_refuses_replay
handlers::api::caches::retained_probe_tests::legacy_uri_type_conversion_strips_inherited_auth
queries::cache_destinations::atomic_scope_tests::create_scope_failure_leaves_no_cache_credentials_or_global_fallback
queries::cache_destinations::atomic_scope_tests::update_scope_failure_preserves_entire_config_ciphertext_and_assignments
queries::cache_destinations::tests::niks3_selection_assigned_first_disabled_fallback_and_stable_order
queries::cache_destinations::tests::niks3_assignment_writers_wait_for_publication_snapshot
handlers::agent::heartbeat::tests::niks3_signed_handler_preserves_pending_deployment_and_delivers_only_selected_reads
handlers::api::builders::tests::niks3_builder_and_agent_selection_share_assigned_first_policy
queries::builders::tests::niks3_canonical_environment_dispatch_rejects_ambiguity_before_credentials
queries::builders::tests::niks3_canonical_environment_completion_rejects_post_dispatch_ambiguity
queries::builders::tests::niks3_preclaim_handler_rejects_legacy_and_allows_capable_builder
handlers::api::builders::proxy_dispatch_tests::proxy_credential_dispatch_signed_tcp_next_job
queries::builders::tests::niks3_exact_candidate_claim_never_substitutes_after_queue_races
queries::builders::tests::niks3_dispatch_identity_and_completion_transaction_rechecks
queries::builders::tests::niks3_missing_push_queues_exact_id_and_requires_authoritative_output
queries::builders::tests::niks3_pending_publication_guards_rollback_and_later_promotion
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
builder::cve_worker::tests::materialization_completed_provenance_identity_and_eligibility
handlers::api::caches::tests::niks3_api_create_rejects_malformed_keys_in_all_auth_modes
handlers::api::caches::tests::niks3_api_update_rejects_malformed_keys_in_all_auth_modes
```

The four TASK-488 isolated mock tests use migration-backed disposable SQLx
databases. The original two persistence tests verify exact-attempt outcomes and
UUID-keyed policy details in `evaluation_mock_outcomes`, not the generic
derivation writer. Repeated writes preserve first-write evidence; updates and
invalid attempt ownership or markers are rejected. Cancelled or superseded
attempts write no mock outcomes. Mock persistence creates no derivations,
buildable rows, build jobs, configuration-observation requests or observations,
or composite assessments; both tests assert unchanged counts for these rows.

The queue/history test verifies matching isolated mock counts, exclusion of old
mock evidence from a queued retry, and native-only counting when a native retry
starts. The projection test verifies exact-attempt mock evidence, rejects invalid
markers, preserves Unknown for missing results, and withholds outcomes and global
evidence when current native and mock evidence are ambiguous. A queued retry does
not inherit old mock results. These two tests insert native derivation fixtures
explicitly to exercise coexistence and ambiguity.

The three mock persistence tests are declared in `models/mock_persistence.rs`,
loaded by `#[path = "mock_persistence.rs"]` and `mod mock_persistence;` inside
`models::evaluate_with_policies`. The projection test is declared in the `tests`
module of `queries/eval_policy_plans.rs`. The qualified names above follow these
module declarations. Membership does not establish a passing result for the
current source.

The four derivation-count tests are declared in `queries::commits::tests` with
ignored, migration-backed SQLx test attributes. Queue and history counts report
zero persisted derivation records for isolated current mock outcomes, exclude
stale native records, and count current native records across statuses rather
than result groups. A queued retry does not inherit the previous attempt's count.
Markerless or malformed legacy markers make the count unavailable even when a
known current record exists. These declarations describe coverage; membership
does not establish a passing result for the current source.

The two native-finalizer tests are declared in `queries::eval_policy_plans::tests`
with ignored, migration-backed SQLx test attributes. They invoke the real
`finalize_evaluation_attempt` with confirmed Nix failures. The failure test covers
new derivations, resettable evaluation rows, and irreversible build state. It
checks the exact-attempt failure marker and diagnostic, flat `eval_failed`
projection, and structured controls that remain Unknown without policy details.
Irreversible build status and derivation/store paths survive the failure stamp.
Malformed failure flags or diagnostics and stale attempt markers are masked;
a queued retry also hides the previous failure diagnostic and policy evidence.
The cancellation test compares complete derivation rows before and after the
finalizer for both absent and existing derivations. Cancellation writes no
failure evidence, preserves existing rows, and creates no evaluation snapshots.
Each fixture flake has a distinct repository URL so independent scenarios do
not collide with the uniqueness constraint. Execute the qualified cases through
the guarded runner; registration alone does not establish a passing result.

The partial resource-failure tests use migration-backed disposable SQLx databases.
They preserve exact completed derivation, build-job, assessment, and rule rows;
publish a captured snapshot for the completed configuration; and mark only
unfinished configuration evidence as Error with `failure_code: resource_pressure`.
The code is a compatibility marker for stopped bounded recovery, not proof of
physical OOM. Persistence acknowledgement can be unavailable with an unknown
infrastructure cause. Operator guidance therefore requires checking evaluator
resource diagnostics and database acknowledgement, then resolving the recorded
cause before manual retry. Cause classification does not parse diagnostic text.
Configurations outside the selected remainder keep their prior lifecycle and
do not contribute to the partial attempt's failure summary.
Checked agent-policy rejection remains intact. Commit and attempt become failed,
with no whole-flake retry even when the normal retry budget permits five children.
Forged paths and wrong-attempt markers roll back all terminalization writes.
Superseded and cancelled reports make no changes. These tests do not invoke Nix
or use a host development database.

The preserved-history policy-rejection regression uses two real evaluation
attempts and production persistence. A strict policy failure in the second
attempt cancels an older queued job and preserves an older building job.
The same regression covers passing and policy-rejected completions with queued
or pre-0195 `NULL` preparation state, including building and terminal history.
Both completed evaluations retain exact current-attempt metadata, compatible
preparation history, policy Fail assessments, and captured snapshots through
resource terminalization. The finalizer creates no job or automatic child.
Rejected new pending preparation is still invalid, and strict job admission
still refuses the rejected result. Historical completion is not new-work
eligibility. `NULL` without compatible history cannot prove completion or admit
a rejected job. The orphan selector does not select `NULL` rows for new rooting;
an eligible legacy row with existing history can only report that job idempotently.

Resource-terminal recovery acquires a new bounded preparation lease before
its first GC-root probe. Database and heavy-Nix permits remain owned through
rooting and confirmed cleanup. A quarantined evaluator group prevents overlap;
the root operation uses the runtime helper's 120-second deadline, including
lock waits. Candidate state is revalidated after rooting. Legacy complete-commit
recovery keeps its existing root path.

The committed-but-unacknowledged regression asserts that runtime catch-up is
read-only and uses exact current-attempt identity, output, policy evidence,
and preparation eligibility. Terminal handoff retains the committed capture
and Pass evidence while leaving preparation pending and creating no job
before GC rooting. The never-committed candidate regression asserts that a
candidate cannot create a derivation, job, or available snapshot. The pure
diagnostic regression asserts that transaction-proven counts and unfinished
names replace checkpoint guesses and exclude reconciled completions from
resource-failure observations. These assertions describe gate coverage;
the current PostgreSQL and full-gate results remain parent-owned and pending.

The mixed-cohort regression keeps completed evaluation, confirmed Nix failure,
and unresolved resource failure disjoint. It terminalizes their selected union
without requiring confirmed failures in the unresolved-resource list. Completed
Pass, snapshot, and build job survive; the confirmed failure keeps its Nix
diagnostic and ConfirmedFailure evidence; only resource work receives resource
Error. Unselected configurations remain untouched.

The pending-preparation regression leaves a validated successful derivation
pending through resource terminalization. It proves exact current-attempt
recovery eligibility and guarded backoff, rejects ordinary failure, malformed
resource prefixes, stale or missing markers, rejected policy, cancellation,
scope-excluded or queued-without-job state, archived sources, wrong commit
identity, and newer attempt lineage. It exercises
the actual database activation phase and proves one job across repeated calls.
The fixture assumes the activation caller's GC-root proof; it does not invoke
Nix or claim a real store/root verification. The production recovery worker
still requires successful rooting before transactional admission.

The two legacy Attic tests insert historical columns directly, bypassing the new
create path. Plaintext and independently constructed historical `enc:v1` rows
must produce matching single/list configured flags with no returned token.
The envelope uses AES-256-GCM, a SHA-256-derived runtime fixture key, a 12-byte
nonce, empty additional authenticated data, and standard Base64 nonce and
ciphertext with a 16-byte appended tag. Stored-ID Test and Cancel without a PUT
preserve exact raw rows and assignments. Explicit blank same-type Save fails
without mutation; omitted-token unrelated Save preserves raw credentials and
URLs, allowing only the requested name, `updated_at`, and assignment `created_at`
changes with exact membership. NULL and empty tokens fail before the probe
callback. Synthetic JWT-shaped Rust values prove retained snapshots; actual
native Bearer authentication remains the separate owner-managed VM proof.

Attic Test uses the canonical named-cache `/_api/v1/cache-config/<cache>` API.
Server-root HTTP 200, HTML, arbitrary JSON, invalid keys, missing required fields,
and invalid store directories cannot establish cache access. Required native
fields are `public_key` (valid Nix key), `is_public` (boolean), `store_dir`
(`/nix/store`), and `priority` (i32). The probe bounds metadata to 64 KiB and
retains HTTPS, pinned DNS, verified TLS, no-proxy, no-redirect, and eight-second
timeouts. Attic read configuration uses the same resolver's named-cache root.

Results add flat `probe_kind: "attic_cache_config"`, `stage`,
`cache_access_valid`, `cache_is_public`, `token_auth_valid`, and
`write_auth_valid` fields. Stages
are `target_policy`, `dns`, `transport`, `authentication`, `cache_not_found`,
`response`, and `complete`. Access is null before HTTP observation, false after
an unsuccessful response, and true only for validated native metadata. Token
validity is true only for private-cache success and null otherwise. Write
validity is always null (Untested). Native `cache_is_public` is a boolean only
after complete metadata validation; failed or incomplete probes leave it null.
Public read access does not validate a supplied token or establish write access.
The existing exact classification test covers these fields without adding a new
test name. Its updated assertions await final-head CI execution.
HTTP 401/403 leaves existence unresolved;
only HTTP 404 with typed `code: 404, error: "NoSuchCache"` reports cache absence.
Generic 404 reports `endpoint_unavailable` at the response stage. Target-policy
rejection remains HTTP 400 with safe `error`, `message`, and null `details` plus
the flat probe fields. Other probe outcomes remain HTTP 200 result objects.
No upstream metadata, URLs, tokens, or error text are returned or followed.
The PostgreSQL regression checks retained/replacement snapshots, credential-safe
results, lock release, and exact raw-state preservation on policy refusal.

The requisite-publication regression verifies that `ATTIC_SERVER_URL` uses the
shared resolver's canonical server base for Attic.

Niks3 Add discovery accepts a backwards-compatible URL-only request or explicit
write mTLS transport. `server_url` is required; optional string fields are
`niks3_write_auth_mode`, `niks3_write_client_cert`, `niks3_write_client_key`, and
`niks3_write_ca_cert`. Omitted mode means the public/token metadata path with no
TLS fields. mTLS requires a complete certificate/key pair and permits an optional
CA bundle. Strict certificate-only PEM validation accepts multiple certificates
and rejects empty input, private keys, comments, trailing text, and malformed
certificates before DNS. Key parsing errors never include key or parser contents.
The server HTTP client adds supplied CA trust to its system roots; this does not
change the packaged CLI's separate CA semantics.

`POST /api/v1/caches/:id/niks3/discover` accepts the exact unwrapped Save update
shape. Admin authorization precedes parsing and lookup; supplied CSRF state must
match. The handler loads and decrypts one unlocked snapshot, then uses the full
shared effective-update validator. Tests prove retained/replacement credentials,
CA clearing, mode transitions, lock release, safe refusals, and exact raw database
non-mutation. Configured presentation flags cannot select retained credentials.
No assignment, timestamp, usage, ciphertext, or job write occurs.

Both discovery forms return only `server_url`, `substituter_url`, `public_keys`,
and nullable `oidc_audience`. Write mTLS is used only for the configured metadata
endpoint; read credentials and Bearer tokens are never sent. HTTPS/private-target
policy, all-address DNS validation and pinning, hostname verification, no proxies,
no redirects, eight-second timeouts, and the 64 KiB body limit remain enforced.
The pinned Niks3 v1.6.0 `GET /api/cache-config` is public metadata, not a write
authorization check. Discovery success leaves write authorization Untested;
discovery does not invoke the protected write-route probe. Actual native TLS
acceptance is covered by the separate owner-managed VM fixture; the discovery
regressions prove transport projection and state.

The 21 Niks3 write-probe regressions guard the supported v1.6.0/v1.8.0 handler
contract without changing the production package pin. Exact status, media type,
body fingerprints, and typed GC snapshot fields must match. A successful protected
GET and a discriminating credential-omission control permit one invalid `{}` POST
to `/api/pending_closures` plus its control. Unknown GET fingerprints prohibit
POST. No GC-start request or valid closure is sent. Unexpected POST success is
inconclusive and is not retried. Successful evidence proves protected-handler
access only, not S3 permission, signing, upload, or verified publication.

Tests cover selected versus omitted credentials, mTLS subject denial versus a
specific certificate-gate rejection, retained/replacement write snapshots,
metadata/read credential isolation, bounded chunked bodies, shared deadlines,
generic proxy responses, and contradictory controls. Missing or invalid local
write-only tokens report false authentication and authorization before DNS.
Full `all` scope retains its existing local missing-token validation refusal.
Invalid Bearer headers in `all` scope fail before metadata DNS or read requests.
Token-free public metadata reachability remains independent of protected
authentication. A metadata signing-key mismatch in `all` scope does not hide API
reachability or skip the protected authentication check. Metadata-only evidence
leaves write authorization inconclusive. Explicit read CA bundles replace ambient
roots; invalid or empty bundles fail without fallback to system trust. These
client-construction assertions do not prove native TLS handshakes.
Provider bodies and error prose never become diagnostics. These classifier and
scheduled-response tests do not prove native acceptance. The blocking
[native write-probe check](../niks3-write-probe/README.md) covers real retained
encrypted CF rows, native handlers, and non-mutation; execution remains pending CI.

Basic read regressions cover encrypted password storage, management redaction,
configured flags, complete pair replacement, mode clears, and HTTPS authority
binding. GET, Test, Cancel, and failed Save preserve raw state. Unrelated Save
preserves retained ciphertext. Delivery requires the signed Basic-read capability
and confidential transport; old or insecure agents receive neither target nor
source and retain pending work. The publication fingerprint test includes Basic
credential changes alongside the existing policy and canonical-set assertions.
Independent write/read probes do not borrow the other plane's credentials.

The 18 metrics tests distinguish native values from unavailable measurements.
Missing, denied, redirected, malformed, negative, and overflow responses cannot
become zero-valued statistics. Metrics retain their reported byte/object basis
without inventing path counts. Disabled, unsupported, non-admin, and anonymous
requests do not probe. The tests also cover bounded bodies, cancellation at the
overall deadline, configured-authority requests without authorization headers,
explicit null fields, and `Cache-Control: no-store`. These unit tests do not prove
native-provider acceptance or represent real account/storage measurements.

The nine Attic-specific tests cover exact cache identity, unsigned 64-bit totals,
optional configuration gates, HTTPS mapping validation, adapter-only Bearer
projection, protected token files and alphabet boundaries, bounded responses,
and refusal before file or network access. Native restricted-role and module
lifecycle acceptance remains covered by the separate `attic-metrics` VM check.

The following 83 existing non-ignored tests and the nine TASK-488 pure tests
described above run with `--exact --test-threads=1`, for 92 in total:

```text
queries::commits::partial_resource_failure_tests::partial_failure_diagnostic_uses_proven_counts_not_checkpoint_guesses
handlers::api::builders::tests::evaluator_fixture_reproduces_inherited_cloexec_writer
handlers::api::builders::tests::evaluator_fingerprint_reports_executing_nix_and_pure_contract
queries::commits::partial_resource_failure_tests::evaluation_failure_resource_marker_is_server_owned
queries::commits::partial_resource_failure_tests::partial_failure_cohort_keeps_confirmed_and_resource_systems_disjoint
models::retry_policy::tests::resource_failure_never_retries_whole_flake_despite_available_budget
models::cache_destination::tests::niks3_basic_pair_validation_redaction_modes_and_authority
models::cache_destination::tests::niks3_probe_planes_bootstrap_without_unselected_settings
handlers::api::caches::basic_read_tests::niks3_basic_http_projection_and_plane_scope_do_not_cross_credentials
handlers::api::caches::niks3_write_probe::tests::native_gc_schema_rejects_generic_json_and_invalid_typed_fields
handlers::api::caches::niks3_write_probe::tests::exact_status_media_type_and_body_are_required
handlers::api::caches::niks3_write_probe::tests::mtls_control_requires_a_specific_identity_alert_not_any_transport_failure
handlers::api::caches::niks3_write_probe::tests::nginx_omission_requires_complete_page_and_only_opens_mtls_control
handlers::api::caches::niks3_write_probe::tests::nginx_ingress_control_still_requires_both_native_handler_fingerprints
handlers::api::caches::niks3_write_probe::tests::native_token_route_pair_proves_access_and_sends_only_invalid_bodies
handlers::api::caches::niks3_write_probe::tests::no_post_follows_unknown_get_or_non_discriminating_authentication_control
handlers::api::caches::niks3_write_probe::tests::definite_native_token_denial_is_false_not_untested
handlers::api::caches::niks3_write_probe::tests::strict_gc_json_accepts_finished_failure_without_exposing_provider_error
handlers::api::caches::niks3_write_probe::tests::mtls_route_requests_send_no_bearer_and_native_subject_denial_is_false
handlers::api::caches::niks3_write_probe::tests::retained_and_replacement_effective_snapshots_select_only_active_write_credentials
handlers::api::caches::niks3_write_probe::tests::missing_or_invalid_write_token_reports_local_failure_without_dns_or_read_borrowing
handlers::api::caches::niks3_write_probe::tests::all_scope_invalid_bearer_preflight_precedes_metadata_dns_and_read_requests
handlers::api::caches::niks3_write_probe::tests::niks3_read_custom_ca_replaces_ambient_roots_and_invalid_bundles_do_not_fallback
handlers::api::caches::niks3_write_probe::tests::write_metadata_reachability_is_independent_of_protected_authentication
handlers::api::caches::niks3_write_probe::tests::all_metadata_key_mismatch_keeps_api_reachable_and_does_not_skip_authentication
handlers::api::caches::niks3_write_probe::tests::all_scope_missing_token_keeps_existing_local_validation_rejection
handlers::api::caches::niks3_write_probe::tests::generic_post_errors_and_contradictory_controls_never_verify_authorization
handlers::api::caches::niks3_write_probe::tests::unexpected_success_stops_without_control_retry_or_valid_request
handlers::api::caches::niks3_write_probe::tests::metadata_and_read_requests_do_not_receive_write_bearer
handlers::api::caches::niks3_write_probe::tests::chunked_bodies_are_bounded_and_a_shared_deadline_covers_stream_consumption
security::cache_secrets::tests::basic_password_envelope_preserves_spaces_and_literal_prefixes
queries::builders::tests::niks3_publication_fingerprint_includes_secrets_policy_and_canonical_sets
handlers::api::caches::storage_metrics::tests::native_values_preserve_basis_without_inventing_paths
handlers::api::caches::storage_metrics::tests::attic_inventory_preserves_exact_identity_u64_and_path_basis
handlers::api::caches::storage_metrics::tests::attic_inventory_rejects_missing_null_negative_overflow_and_identity_mismatch
handlers::api::caches::storage_metrics::tests::attic_optional_metadata_gate_requires_current_enabled_type_and_admin
handlers::api::caches::storage_metrics::tests::attic_mapping_is_explicit_https_and_fails_closed_for_every_invalid_entry
handlers::api::caches::storage_metrics::tests::attic_request_uses_only_adapter_bearer_and_exact_canonical_name
handlers::api::caches::storage_metrics::tests::attic_token_loader_rejects_symlinks_public_files_and_bad_tokens
handlers::api::caches::storage_metrics::tests::attic_protected_token_file_enforces_exact_adapter_alphabet_and_boundaries
handlers::api::caches::storage_metrics::tests::attic_response_is_bounded_and_status_errors_are_static
handlers::api::caches::storage_metrics::tests::attic_invalid_mapping_or_name_stops_before_file_or_network
handlers::api::caches::storage_metrics::tests::malformed_negative_missing_and_overflow_stats_never_become_zero
handlers::api::caches::storage_metrics::tests::metadata_gates_skip_all_unsupported_disabled_and_non_admin_probes
handlers::api::caches::storage_metrics::tests::missing_endpoint_denial_redirects_and_non_native_success_are_not_empty_stats
handlers::api::caches::storage_metrics::tests::streamed_body_limit_rejects_overflow_without_retaining_rejected_chunk
handlers::api::caches::storage_metrics::tests::absent_metrics_wire_shape_is_explicit_and_not_http_cached
handlers::api::caches::storage_metrics::tests::stats_request_keeps_configured_authority_prefix_and_no_authorization
handlers::api::caches::storage_metrics::tests::overall_deadline_cancels_pending_work_and_returns_nulls
handlers::api::caches::storage_metrics::tests::unauthenticated_request_stops_before_database_lookup
handlers::api::caches::discovery_tests::niks3_discovery_projects_only_write_mtls_and_public_mode
handlers::api::caches::discovery_tests::niks3_discovery_rejects_invalid_bundles_and_queries_before_network
handlers::api::caches::discovery_tests::niks3_discovery_admin_first_and_public_response_redact_tls_material
security::cache_secrets::tests::certificate_bundle_regressions_reject_interleaved_comments_and_noncert_blocks
handlers::api::caches::niks3_ca_tests::ca_normalization_keeps_roots_separate_from_strict_client_chains
handlers::api::caches::niks3_ca_tests::ca_diagnostics_are_typed_static_and_field_scoped
handlers::api::caches::niks3_ca_tests::mismatched_write_identity_fails_before_dns_and_never_proves_auth
handlers::api::caches::niks3_ca_tests::custom_write_roots_skip_ambient_trust_and_validate_every_root
handlers::api::caches::niks3_ca_tests::transport_diagnostics_use_local_causes_not_url_or_body_text
queries::cache_destinations::tests::retained_niks3_secrets_preserve_exact_opaque_storage
queries::cache_destinations::tests::effective_niks3_secret_clears_discard_original_storage
queries::cache_destinations::tests::ca_updates_retain_all_niks3_secret_storage_and_inactive_fields
handlers::api::builders::tests::attic_requisite_env_uses_shared_server_base
handlers::api::caches::retained_probe_tests::attic_canonical_api_and_model_read_roots_share_named_cache
handlers::api::caches::retained_probe_tests::attic_native_metadata_and_status_matrix_never_claims_root_or_write_success
handlers::api::caches::retained_probe_tests::attic_target_policy_and_credential_queries_fail_before_network
handlers::api::caches::retained_probe_tests::active_type_metadata_and_serialization_never_reveal_inactive_credentials
handlers::api::caches::retained_probe_tests::effective_update_never_borrows_inactive_credentials_on_type_conversion
handlers::api::caches::retained_probe_tests::bearer_auth_is_exclusive_to_active_attic
handlers::api::caches::retained_probe_tests::every_url_field_redacts_decoded_aws_queries_and_invalid_urls_fail_closed
handlers::api::caches::s3_probe::tests::sigv4_signs_exact_bucket_host_path_query_and_sensitive_session
handlers::agent_request::tests::niks3_capability_requires_authenticated_body_and_ignores_unsigned_headers
handlers::agent::heartbeat::tests::niks3_selected_cache_never_drops_private_or_unsupported_first_for_fallback
handlers::api::builders::tests::niks3_preclaim_capability_gate_preserves_legacy_cache_dispatch
handlers::api::builders::tests::cache_probe_failure_codes_and_wire_contract_are_static
handlers::api::builders::tests::cache_probe_exact_root_refresh_attic_and_private_capture_bounds
handlers::api::builders::tests::niks3_basic_probe_applies_guard_to_path_info_only
handlers::api::builders::tests::niks3_completion_probe_uses_independent_read_plane
handlers::api::builders::tests::niks3_probe_failure_never_polls_success_transition
handlers::api::builders::tests::niks3_probe_timeout_and_cancellation_reap_before_cleanup
handlers::api::builders::tests::niks3_pinned_nix_accepts_remote_metadata_and_refresh_flags
handlers::api::builders::niks3_input_owner_tests::niks3_input_owner_acknowledges_reap_and_cleanup_after_detach
handlers::api::builders::niks3_input_owner_tests::fifo_deadlines_cover_missing_readiness_and_absent_release_reader
```

### Completion publication contract

Niks3 and Attic completion run `nix path-info --refresh --store <read-store>
<exact-output>` for one authoritative root. The probe does not traverse the
closure, import NARs, or create a local logical store. Niks3 uses protected
read-only credentials and isolated HOME/config/cache directories with mode 0700.
The exact runtime must support the native Basic authority guard before credential
files are created. Configured signing keys and `require-sigs=true` remain settings;
path-info is availability evidence, not NAR integrity or signature proof.
Deployment and CVE materialization retain their existing copy-time checks.
The metadata deadline is 60 seconds, allowing bounded reporting retries.
Private stdout and stderr are each bounded to 64 KiB; only static failure codes
leave classification. Quoted URLs and response bodies cannot supply retry hints.

Successful build completion with a failed probe returns HTTP 202:

```json
{"build_status":"success","publication_status":"pending","verification_code":"cache_path_missing","retryable":true,"retry_after_secs":2}
```

The verification codes are exactly `cache_path_missing`,
`cache_authentication_failed`, `cache_rate_limited`, `cache_network_error`,
`cache_probe_timeout`, `cache_read_configuration_invalid`, and `cache_probe_failed`.
Missing paths can reflect publication visibility races. Missing paths, rate
limits, network failures, and timeouts are retryable with `retry_after_secs: 2`.
Authentication and configuration failures require operator correction; they and
unknown process failures are non-retryable and omit `retry_after_secs`.
Classification does not authorize completion. Invalid output, dispatch ID,
session, status, eligibility, or fingerprint retains the existing hard 403/409
behavior and cannot become pending success.

The atomic pending helper stores exact output and build success plus the captured
destination's durable queue identity. It does not record completed publication.
The GC root and log channel remain owned until a later successful probe returns
HTTP 200 `{"build_status":"success","publication_status":"confirmed"}`.
Completion without a reported push returns HTTP 200 with publication `none`.
Older clients that accept any 2xx can ignore the additive body. Same-claim retries
promote the existing publication row without another build attempt. A failed
current probe never demotes historical completed evidence. The queue remains
operator-visible after reporting retries end; automatic worker recovery is not
guaranteed.

The pending PostgreSQL member proves session/builder/output/dispatch/fingerprint
rollback, exact pending identity/output, later promotion, historical-proof
retention, one build attempt, and bounded server-owned terminal diagnostics.
Builder terminal-log authorization remains closed. The process members prove
exact-root refresh, no copy execution, independent read mTLS/CA and Basic
credentials, private stream bounds, static wire hints, cancellation cleanup,
and the pinned Nix metadata flags.

The input-publication owner regression uses FIFO handshakes to cover success,
child failure, timeout, caller detach, spawn failure, and configuration failure.
Credentials remain available while the child runs. The completion callback
observes child reaping and credential-directory cleanup before it acknowledges
completion. Caller detach does not cancel the process owner.
The FIFO deadline regression proves that missing readiness and a full release
FIFO fail with `TimedOut` rather than leave a blocking fixture operation.

## Execution progress and failure evidence

The check emits timestamped `START` and `END` boundaries for the overall check
and its migration, critical integration, selected regression, and TASK-470
phases. `END` records the actual exit status and elapsed whole seconds. The
timing trap runs in a subshell so the PostgreSQL hook retains cleanup ownership.
Failure stops the check and closes the active phase with the failed status.

Each of the same eleven critical integration targets now has a named `compile`
invocation (`cargo test --no-run`) followed by a named `run` invocation. Both use
`--offline --package cf-server --test <target>`; execution retains
`--test-threads=1`. Cargo reuses shared build artifacts. A last `START` without
its matching `END` identifies the active target and whether Cargo was compiling
or executing. The library also has an explicit compilation boundary before its
selected tests execute. Cargo's normal output reports executed test names.

Each of the 125 ignored and 83 non-ignored selected library tests emits its qualified name,
mode, start time, exit status, and elapsed time. Cargo stdout and stderr still
pass through `tee` to the exact-test guard. `pipefail` preserves failures from
Cargo or `tee`; success still requires the named `... ok` line and the existing
one-passed/zero-failed/zero-ignored summary. Zero executed tests fail the gate.

Progress labels contain only phase names, target names, and modes. The runner
does not print command arguments or environment values, enable `--nocapture`,
or emit periodic heartbeat output. Successful test payload output remains
captured by Rust's default test harness. Failed tests retain existing Cargo
failure output.

CI job `16957035199` last reported compilation warnings at `04:14:07` after
entering the aggregate critical integration command. That trace has no observed
test start and does not identify a hung target, an active PID, or lost output.
The later TASK-470 Attic tests had not been logged. The new boundaries narrow a
future reproduction; they do not establish the cause of that remote failure.

## Run it

```sh
nix build .#checks.x86_64-linux.server-regressions --print-build-logs
```

For a constrained local reproduction with the public binary cache:

```sh
bash packages/ci/public-cache-build.sh --no-link -L --max-jobs 1 --cores 2 \
  --keep-failed --print-out-paths .#checks.x86_64-linux.server-regressions
```

The wrapper validates the public-cache policy with the same CLI options passed
to the build. It preserves inherited builder settings and requires signatures.
The policy applies to this command only. Connection and stalled-download
timeouts bound substitution attempts; they do not change the test or CI job
budget. Retain raw build logs, the command's exit status, and start/finish times
under an external supervisor when the calling terminal has a shorter timeout.
A previously realized derivation is not proof that a changed runner executed;
confirm the new derivation was built and emitted its check boundaries.

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
