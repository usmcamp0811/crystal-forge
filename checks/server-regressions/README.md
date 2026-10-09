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

The following 42 ignored PostgreSQL tests run with
`--ignored --exact --test-threads=1`. Each invocation must report exactly one
passed test and zero ignored tests. The tests include completed CVE publication
provenance and malformed signing-key rejection across all Niks3 auth modes.

```text
handlers::api::caches::basic_read_tests::niks3_basic_get_save_test_cancel_modes_and_authority_preserve_secrets
queries::cache_publication_reads::tests::niks3_basic_publication_withholds_old_capabilities_and_insecure_delivery
handlers::api::caches::discovery_tests::niks3_stored_discovery_retains_replaces_clears_modes_without_persistence
handlers::api::caches::discovery_tests::niks3_stored_discovery_auth_invalid_json_type_tls_and_csrf_fail_without_mutation
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
`cache_access_valid`, `token_auth_valid`, and `write_auth_valid` fields. Stages
are `target_policy`, `dns`, `transport`, `authentication`, `cache_not_found`,
`response`, and `complete`. Access is null before HTTP observation, false after
an unsuccessful response, and true only for validated native metadata. Token
validity is true only for private-cache success and null otherwise. Write
validity is always null (Untested). HTTP 401/403 leaves existence unresolved;
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
connection Test's `write_auth_valid` remains null. No alternative GC/auth probe
or upload is attempted. Actual native TLS acceptance is covered by the separate
owner-managed VM fixture; these regressions prove transport projection and state.

Basic read regressions cover encrypted password storage, management redaction,
configured flags, complete pair replacement, mode clears, and HTTPS authority
binding. GET, Test, Cancel, and failed Save preserve raw state. Unrelated Save
preserves retained ciphertext. Delivery requires the signed Basic-read capability
and confidential transport; old or insecure agents receive neither target nor
source and retain pending work. The publication fingerprint test includes Basic
credential changes alongside the existing policy and canonical-set assertions.
Independent write/read probes do not borrow the other plane's credentials.

The nine metrics tests distinguish native values from unavailable measurements.
Missing, denied, redirected, malformed, negative, and overflow responses cannot
become zero-valued statistics. Metrics retain their reported byte/object basis
without inventing path counts. Disabled, unsupported, non-admin, and anonymous
requests do not probe. The tests also cover bounded bodies, cancellation at the
overall deadline, configured-authority requests without authorization headers,
explicit null fields, and `Cache-Control: no-store`. These unit tests do not prove
native-provider acceptance or represent real account/storage measurements.

These 32 non-ignored tests run with `--exact --test-threads=1`:

```text
models::cache_destination::tests::niks3_basic_pair_validation_redaction_modes_and_authority
models::cache_destination::tests::niks3_probe_planes_bootstrap_without_unselected_settings
handlers::api::caches::basic_read_tests::niks3_basic_http_projection_and_plane_scope_do_not_cross_credentials
security::cache_secrets::tests::basic_password_envelope_preserves_spaces_and_literal_prefixes
queries::builders::tests::niks3_publication_fingerprint_includes_secrets_policy_and_canonical_sets
handlers::api::caches::storage_metrics::tests::native_values_preserve_basis_without_inventing_paths
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
handlers::api::builders::niks3_input_owner_tests::niks3_input_owner_acknowledges_reap_and_cleanup_after_detach
handlers::api::builders::niks3_input_owner_tests::fifo_deadlines_cover_missing_readiness_and_absent_release_reader
```

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

Each of the 42 ignored and 32 non-ignored TASK-470 tests emits its qualified name,
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
