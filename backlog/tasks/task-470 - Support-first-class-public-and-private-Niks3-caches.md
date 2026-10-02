---
id: TASK-470
title: Support first-class public and private Niks3 caches
status: In Progress
assignee:
  - openai-agent
created_date: '2026-10-02 22:23'
updated_date: '2026-10-02 23:08'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Preserve origin/dev baseline in dedicated TASK-470 worktree. Use current cf-config re-export rather than obsolete server config copies.
2. Add additive protocol types CacheReadAuth (None/Mtls) and Niks3WriteAuth (Token/Mtls), redacted Debug and defaults. Add shared cf-config credential preparation owning protected temporary files through child completion; share it for push commands and Nix read URL/CA/key preparation.
3. Add next-free migration and Niks3 model/DTO fields. Extend existing encryption/decryption/update layer with explicit clear flags and atomic mode-transition cleanup. API responses redact all private material and expose configured flags.
4. Wire builder and server local pushes to one shared Niks3 command preparation; avoid AWS credentials and multiplied per-job concurrency. Extend destination ID completion and confidentiality checks; preserve server-side Nix readability verification.
5. Wire agent heartbeat read-only auth with verified transport checks, deployment pull, and CVE materialization to the shared read helper.
6. Add protected discovery and non-mutating structured connectivity checks; disable redirects or validate each hop using existing target policy.
7. Start task-owned devenv preview with isolated DB and mock fixtures before UI changes. Update design plus create/edit/discovery/auth UI and behavior tests.
8. Package CLI preserving evaluator Nix PATH; add focused Niks3 VM fixture/check covering all required flows and regressions. Prepare SQLx only against verified task DB. Review documentation and raw verification results before commit/push/MR.
Initial static-token and mTLS scope is approved. External providers and Bearer/OIDC reads are not implemented in this task.

Delegated execution scope: cf-builder crate and cf-server/src/derivations/{cache,utils}.rs only. Use PreparedNiks3Push for one CLI process per job with parallel_uploads; suppress child output and retain credential ownership through exit or timeout kill/reap. Map server-supplied Niks3 fields explicitly and return destination ID only after an actual successful push. Verify owned Rust files and targeted builder checks via nix develop; protocol/config/helper implementation is owned by another agent.

Delegated API scope: packages/default/crates/cf-server/src/handlers/api/caches.rs and discovery route wiring in src/bin/server.rs only. Verify Niks3 v1.6.0 source GET /api/cache-config shape; add admin discovery; reuse private-target policy with pinned DNS, no proxy and no redirects; add structured non-mutating discovery/read tests using separate write/read TLS credentials and report write_auth_valid null because public cache-config cannot prove authorization. Explicitly redact Niks3 secrets after refreshing configured flags. Run focused handler tests and owned-file formatting through nix develop. No commits or services.

Delegated Nix/test scope: preserve pinned pkgs.niks3 1.6.0; override its Nix dependency to evaluatorNix and add server/core-server/builder runtime packaging plus service PATH entries. Extend builder-evaluator-packaging probes. Add isolated PostgreSQL/Garage/Niks3 fixture with optional TLS/mTLS write/read proxies, dedicated niks3-cache VM auth matrix, cf-test-suite additions and documented focused commands. Evaluate and build targeted checks only; do not run broad flake check. Shared worktree and task lock remain owned by parent agent.

Delegated server/agent scope: builders handler, agent heartbeat, local cache worker, CVE worker and cf-agent only; obsolete server deployment copy only for compile fixes. Reuse existing trusted_proxy_cidrs with actual ConnectInfo direct peer plus trust_forwarded_builder_https and a single HTTPS assertion; missing peer fails closed. Map Niks3 explicitly with independent read/write credentials; validate completion against derivation-eligible enabled destinations with mandatory Niks3 ID. Prepare shared read credentials inside process-owned async tasks through exit/timeout cleanup; pass multiple keys and child-local CA while preserving signatures. Add focused confidentiality/identity/read-process tests and run targeted cargo checks/tests through nix develop. No commits or service startups.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: openai-agent on sledge in /home/mcamp/code/crystal-forge/TASK-470-niks3-cache-support

Preflight: origin/dev fetched at 4fb7d960909bb7dc75d5f859af43e38f7a3c657e; reviewed MR !329 head 9dda346eaee14c3eb83ad47197ae768f3234cbfc is an ancestor. Dedicated clean branch TASK-470-niks3-cache-support. User explicitly permitted preserving untracked dev/session-ses_f927.md and proceeding without a registered main worktree. Risk: high due to credential transport and persistence. Impact: protocol/config/server/builder/agent/UI/Nix packaging/tests. No outstanding baseline dependency. Verification: targeted Rust/model/command/security tests via Nix shell; isolated SQLx metadata preparation; server builder niks3-cache attic-cache s3-cache web-ui checks; nix flake check --keep-going; VM public/private deployment tests and secret-log audit. No arbitrary token scripts, Bearer/OIDC read proxy, generic cache renaming, or priority refactor.

Delegated builder/local-execution implementation is present in cf-builder/src/{cache.rs,bin/builder.rs,builder/api_client.rs,derivations/cache.rs,derivations/utils.rs} and cf-server/src/derivations/{cache.rs,utils.rs}. Both Niks3 branches use PreparedNiks3Push before generic cache_command. One CLI process per attempt receives parallel_uploads once. Detached process ownership retains credentials through cancellation, kill, and reap; timeout cleanup completes before retry. Child output is discarded, failures retain exit status, and environment allowlisting plus task credential-directory HOME/XDG_CONFIG_HOME prevent AWS/Garage/Attic or ambient Niks3 token inheritance. Builder maps job-supplied write fields without local fallback and reports destination ID in signed completion only after actual publication; mock/no-push completion reports None. No commits or service stacks started. Verification: nix develop -c cargo check --manifest-path packages/default/Cargo.toml -p cf-builder --all-targets passed (exit 0); nix develop -c cargo test --manifest-path packages/default/Cargo.toml -p cf-builder niks3 passed (exit 0, 7 tests); nix develop -c rustfmt --edition 2024 --check on all seven owned changed files passed (exit 0). Fake CLI tests use shared helper preparation and cover flags, token/private-key absence, mTLS HOME isolation, output suppression, timeout kill/reap and cancellation cleanup. Signed HTTP completion test checks destination identity and None without push. Shared protocol/config/helper dependencies are now present and compile with builder. Full server/package/VM integration verification remains with task owner.

Delegated cache API implementation is present only in packages/default/crates/cf-server/src/handlers/api/caches.rs and eight discovery route lines in src/bin/server.rs. Added admin POST /api/caches/niks3/discover plus /api/v1 alias; source validated against Niks3 v1.6.0 server/cache_config.go, api/types.go, and server/server.go. Discovery returns canonical server_url/substituter_url/public_keys/oidc_audience (null absent), validates HTTPS URLs and Nix Ed25519 key shape, resolves both targets under the existing private flag, bounds bodies to 64 KiB, and performs no writes. All cache probes now use validated pinned DNS addresses, no proxy, no redirects, TLS host verification, eight-second request/DNS limits. Niks3 connection result extends existing JSON with server_reachable/discovery_valid/write_auth_valid/read_endpoint_reachable/signing_keys_found. write_auth_valid is always null and write token never sent to public cache-config. Separate write/read mTLS PEM identities and CA bundles are applied. Discovered URL must match configured read URL before credentials are used; GET nix-cache-info must report StoreDir /nix/store. Explicit Niks3 token/key removal follows configured flag refresh in all redacted destination responses. Added focused tests for actual wire shape, invalid JSON/URLs/keys, missing keys, discovery failure stages, redirect status rejection, private/mapped/mixed DNS targets, private flag, TLS input redaction, auth-before-probe, configured flags and legacy response compatibility. New public request/handler and changed test handler contracts have rustdoc; request example added. No commits or services started.

Verification for delegated API scope: nix develop -c rustfmt --edition 2024 --config skip_children=true --check packages/default/crates/cf-server/src/handlers/api/caches.rs packages/default/crates/cf-server/src/bin/server.rs passed (exit 0); git diff --check -- packages/default/crates/cf-server/src/handlers/api/caches.rs packages/default/crates/cf-server/src/bin/server.rs passed (exit 0). nix develop -c cargo test --manifest-path packages/default/Cargo.toml -p cf-server --lib handlers::api::caches::tests failed compilation outside scope: six old two-argument builder_https_verified_by_trusted_proxy test calls in handlers/api/builders.rs and missing Niks3 fields in models/builders.rs CacheConfig initializer. After later test additions, rerun no longer reported the six builder test errors; it reported the remaining models/builders.rs:80 missing niks3_public_keys/niks3_read_auth/niks3_server_url/niks3_write_auth and timed out at 120 seconds before exit. nix develop -c cargo check --manifest-path packages/default/Cargo.toml -p cf-server --bin server failed with the same models/builders.rs:80 error. No compiler diagnostics referenced the owned API files, but no tests or route compile success can be claimed. Owning agent must repair models/builders.rs and rerun handler tests/server check/request doctest; real TLS/read-mTLS/redirect transport behavior remains for owner integration fixtures. Parent task remains In Progress.

Nix/test owner: pinned niks3 is 1.6.0 (inspected pinned nixpkgs package via nixos MCP); override only its nix dependency to nix-eval-jobs.nix. Added server/core-server/builder wrappers and module PATH entries, fixture and five-variant VM check. KVM readable/writable. path:.# niks3-cache drv evaluation passed. Packaging build currently blocked outside owned scope: cf-server/src/models/builders.rs:80 E0063 missing niks3_public_keys, niks3_read_auth, niks3_server_url and niks3_write_auth in CacheConfig initializer. Packaged Niks3 and builder built; VM run not passed. Git .# initially omitted untracked shared cache_credentials.rs; use path:.# until owner tracks all new Rust files.

Server/agent delegated implementation now maps Niks3 explicitly in assigned handlers/workers and cf-agent. Existing cf-config ServerConfig.trusted_proxy_cidrs is usable with real ConnectInfo (server already enables connect-info service); no new config field is required. Secret transport gate now requires actual allowlisted direct peer, opt-in and exactly one X-Forwarded-Proto:https, rejecting missing peer, spoofed headers and protocol chains. Private agent read configs are omitted on unverified transport. Agent focused tests pass (6/6), including public/mTLS multiple signing keys, child-local CA, credentials through cancellation/timeout, output suppression, cleanup and fake ambient write-secret exclusion.

Owner action needed outside delegated ownership: cf-server/src/models/builders.rs:72 BuilderCachePushConfigExt::to_cache_config must explicitly handle CacheType::Niks3 and initialize niks3_server_url, niks3_write_auth, niks3_public_keys, niks3_read_auth. For Niks3, map only job-supplied write URL/auth and read URL/public metadata; do not inherit local write auth or AWS/Attic secrets. Current cargo check -p cf-server --all-targets is blocked by E0063 at models/builders.rs:80, preventing server test execution. No edits made to models/protocol/config.
<!-- SECTION:NOTES:END -->
