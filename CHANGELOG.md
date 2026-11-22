# Changelog

All notable changes to Crystal Forge are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Entries cite the
GitLab merge request (`!N`) that delivered the change.

Crystal Forge versions are Git tags on the default branch. Every workspace
crate carries the same version number.

## [0.4.0] - Unreleased

The release date is set when the `v0.4.0` tag is created. This section covers
the 73 merge requests merged into `dev` after `v0.3.1` (2026-05-30). The
read-only MCP endpoint is counted with !346, which absorbed its slice. Work
that is still open is not listed.

### Upgrading from 0.3.1

- **Database.** The server applies 191 new migrations (`0134` to `0336`) at
  startup. No migration that existed in `v0.3.1` was modified, so stored
  checksums stay valid. Back up the database before you upgrade; the
  migrations have no down scripts.
- **NixOS module.** A comparison of the option declarations in
  `modules/nixos/crystal-forge/default.nix` found no option from `v0.3.1`
  removed. The module gained options for evaluator limits, source delivery,
  remote execution strategy, sessions, and access tokens. The generated
  [option reference](https://crystal-forge.gitlab.io/crystal-forge/) lists
  all of them.
- **Access tokens.** `accessTokens.enable` defaults to `true`. Signed-in users
  can create tokens after the upgrade. Tokens stay inert until a consumer
  admits them. The MCP endpoint is off by default (`mcp.enable = false`) and
  requires `accessTokens.enable`.
- **Upgrade check.** The upgrade check (`integration-upgrade`, scenario
  `version-transition`) starts from the genuine `v0.3.1` packages. See
  [its README](./checks/integration-upgrade/README.md).

### Added

#### Access and identity

- Owner access tokens with a Profile UI (!346, option `accessTokens.enable`, on by default) and
  the authorization groundwork for personal access tokens and the MCP server
  (!345).
- A read-only MCP endpoint at `POST /mcp` with four tools (`list_remediation_work`,
  `get_system_security_context`, `get_remediation_context`, `get_poam_detail`).
  It is off by default, needs `accessTokens.enable`, and arrived in !346
  together with the token service.
- System agent key rotation from the Agent identity section (!319).
- Account notifications and active session management (!314), and the
  Profile and Preferences view (!312).
- Server-owned attention occurrences replace mutable alert-count baselines
  (!307).

#### Compliance and policy

- CF-XCCDF interchange foundation with compliance versioning (!313).
- Cross-framework reusable compliance requirements and requirement-aware STIG
  import (!315).
- Policy enforcement and POA&M workflows (!318).
- Standalone NixOS modules generated from exported policies and bundles
  (`cf-nixos-module`, !317).
- Captured bundle and control evaluation matrix (!334).
- External and reference NixOS configuration assurance, with management modes
  (!340).
- Compliance view: the full view with backend data (!285), then the
  rebuilt bundle table, detail drawer, requirement coverage, and STIG import
  resume (!316). The System Detail Compliance tab
  shows per-control evidence in a drawer (!286).

#### CVEs and scanning

- Scanning view and backend scanning APIs (!267).
- Server-side CVE scan loop with periodic rescans and a background job
  registry (!302), policy-driven scans, and a functional fleet rescan (!321).
- CVE triage and Scanning queue interactions (!329).
- Multi-select filters and the design's export formats in the CVEs view
  (!342).

#### Builds, evaluation, and caches

- First-class public and private Niks3 caches (!331). Agent copy failures are
  classified and a Niks3 signing-key mismatch is reported (!339).
- Verified source re-evaluation as a remote build strategy (!290) and
  end-to-end fixes for remote builders (!299).
- Detection and recording of systems that `nix-eval-jobs` silently drops
  (!309).
- Automatic retry controls and latest-per-flake filtering (!310).
- Infinite scroll in the Builds and Evaluations views (!303).
- Builder side panel with click-to-open flow (!300).

#### Deployment

- Deploy tab flake-position lane, target picker, and flake-position API
  (!343).
- Redacted deployment failure details in history and logs (!344).
- Systems deployment progress and real recent activity (!295), and an
  authoritative `system_events` timeline for Deployment History (!294).
- Agent observability and configurable heartbeat reliability (!293).

### Changed

- The Web UI follows the updated design in every main view: shell and design
  tokens (!269), dashboard (!268, !271, !272) with visibility-scoped
  aggregates (!320), Systems (!273, !275, !274), System Detail history and
  logs (!291), Flakes (!277, !298, !306), Environments (!276), Policies
  (!281), Builds (!278), Evaluations (!279), CVEs (!265, !280), Builders
  (!283, !300), Admin (!282), and the 2026-07 design
  deltas (!301, !304).
- The Flakes view reads from the database only and loads commit history
  lazily (!306).
- The backend is split into process-boundary crates with isolated Nix builds
  (!305).
- The 27 flake checks are replaced by four owner checks: `integration-access`,
  `integration-assurance`, `integration-delivery`, and `integration-upgrade`
  (TASK-490, first delivered in !335). Each flake check has a README (!328).
- The web-ui check is manifest-driven with visual baselines (!292).
- Build invalidation and CI feedback latency are reduced (!324).
- Repository documentation moved into an Open Knowledge Format corpus
  (!332), published with the NixOS options reference on GitLab Pages (!337).

### Fixed

- A builder no longer stays in the building state after a server restart
  (!287).
- Builder API mode registers itself by public key (!288), and the key path
  after `cf-keygen` is correct (!289).
- Remote flake evaluation adds `--impure` and normalizes scp-style Git URLs
  (!296).
- `mkStigModule` no longer double-wraps `mkForce` values in
  `overrideAttrs` (!308).

### Developer experience

- A `devenv` shell with dynamic ports proves parallel-worktree isolation
  (!330), and a fast persistent Web UI E2E loop (!326).
- `AGENTS.md` documents stacked merge requests for large deliveries (!348).

## [0.3.1] - 2026-05-30

Patch release tagged at `f9dd6b9c`. The tag's manifests still reported
`0.3.0`. No separate release notes were written.

## [0.3.0] - 2026-04-16

See the [v0.3.0 release notes](./docs/knowledge/overview/release-notes-v0-3-0.md).
