# Config Explorer Architecture

**Status:** Accepted / implemented initially by TASK-440  
**Scope:** Crystal Forge Config inspection and exploration  
**Related:** TASK-440, MR !323

## Problem statement

Crystal Forge configurations can expose more than 16,000 NixOS options. A
complete option, value, and provenance crawl is expensive in CPU, memory, and
wall-clock time. Deployed evidence included a Stage-1 inspection that reached
the 300-second timeout and a Config job that remained marked `running` while it
waited about 1,432 seconds for the global heavy-Nix advisory lock. An
all-at-once crawl also allows one poisoned branch to reduce the usability of
the whole view.

Users should not need a complete corpus to inspect one setting. Full
configuration extraction is therefore the wrong prerequisite for interactive
browsing.

## Architecture invariants

Crystal Forge keeps three distinct evaluators. Their outputs have different
authority and failure semantics.

### Primary evaluator

The primary evaluator evaluates the NixOS configuration and determines the
authoritative build and deployment pipeline outputs. Its result controls the
evaluation/build/deployment pipeline.

### Policy evaluator

The policy evaluator evaluates deployment and compliance assertions directly
against the exact target configuration. For example, a policy may evaluate
`config.services.openssh.enable == true`. Its result is the authoritative
policy `PASS`, `FAIL`, or `ERROR` result used by deployment gating.

### Config Explorer

The Config Explorer helps a human inspect and understand a configuration. Its
observations are non-authoritative. Config Explorer data MUST NEVER become the
source of truth for deployment policy enforcement.

The following invariants are mandatory:

- Policy evaluation MUST work when no Config Explorer cache exists.
- Policy evaluation MUST work when Explorer data is partial.
- A poisoned Explorer branch MUST NOT change policy semantics.
- Opening Config MUST NOT be required before deployment.
- An Explorer cache hit MUST NOT substitute for an authoritative policy Nix
  evaluation.
- Primary and policy evaluation MUST remain independent of Explorer cache
  completeness, freshness, and availability.

```text
                    Exact NixOS configuration
                             |
             +---------------+---------------+
             |                               |
             v                               v
      Policy evaluator                 Config Explorer
             |                               |
       PASS/FAIL/ERROR                  observational only
             |                               |
      deployment gate             lazy/cacheable inspection
```

## High-level Config path

Config inspection follows the exact target through increasingly narrow work:

```text
exact commit/config/carrier
          |
          v
shallow root inspection
          |
          v
user expands prefix
          |
          v
scoped prefix inspection
          |
          v
user selects option
          |
          v
value/metadata inspection
          |
          v
optional provenance detail
```

The interface is REPL-like in interaction, but it is not an actual REPL. The
initial top-level tree should appear quickly. Nodes expand on demand. Option
details load only when selected. Expensive provenance loads only when
requested or required by the selected detail. Observations for immutable
targets can be cached and reused.

Crystal Forge MUST NOT expose `nix repl` to clients. A persistent client REPL
would create arbitrary-code risk, credential-lifecycle problems, state
contamination, process-lifetime and concurrency problems, and a fragile
protocol boundary. Clients send structured path, depth, and kind requests.
The server builds trusted Nix expressions from validated inputs.

## Inspection phases

The phased model is:

### Phase 0: exact target resolution

Resolve the exact commit or revision, configuration name, and derivation or
carrier. Reject ambiguous, unauthorized, or mismatched targets before Nix
work starts.

### Phase 1: shallow root/bootstrap

Establish a trustworthy top-level hierarchy and basic metadata. This phase MAY
inspect the root structure and bounded diagnostics. It SHOULD avoid evaluating
all option values and all provenance records.

### Phase 2: scoped prefix expansion

Inspect only the requested path prefix and bounded descendants. Expanding a
prefix MUST NOT require evaluating every option value or every provenance
record in the configuration.

### Phase 3: exact option detail

Inspect the selected option's declared type, safe value or error, and basic
metadata. The request remains scoped to the exact option.

### Phase 4: detailed provenance

Inspect definitions, source input, revision, path, and related provenance only
for the selected option or explicitly requested detail. Provenance work MAY be
more expensive than tree navigation and MUST retain the same target identity.

## Target identity and cache contract

Every observation is tied to an immutable identity containing, at minimum:

- commit or revision;
- configuration name;
- exact derivation or carrier;
- path components;
- observation type; and
- inspection schema version.

Results from one carrier or revision MUST NOT be reused for another carrier or
revision. A cache key MUST include every field that can change the inspected
configuration or interpretation of its result.

The cache contract is:

- A cache hit starts zero Nix subprocesses.
- Identical active requests coalesce.
- Different targets run independently.
- A complete V2 snapshot MAY satisfy Explorer reads immediately when its
  authority and completeness contract match the request.

## Explorer observations and certified V2 snapshots

Explorer observations and certified V2 Config snapshots serve different
purposes.

### Explorer observations

Explorer observations are scoped, lazy, potentially incomplete, suitable for
browsing, and non-authoritative. They describe what a bounded request
observed. They do not prove that an uninspected path is absent.

### Certified V2 Config snapshot

A certified V2 snapshot is a coherent inventory artifact. It is complete or
explicitly partial, and it is required for authoritative comparison semantics.
It MAY be generated deliberately or as background enrichment. It remains useful
for Changed, Drift, complete search, and full-corpus analysis.

Explorer observations MUST NOT automatically become a certified snapshot. The
partial-inventory contract preserves these fields and their meanings:

- `option_inventory_complete`;
- `diagnostics`;
- `diagnostics_truncated`; and
- `comparison_ready`.

## Changed, Drift, and search semantics

Changed and Drift require a sufficiently complete certified inventory. Lazy or
partial Explorer data MUST NEVER imply absence:

```text
incomplete data != zero changes
incomplete data != no drift
```

Search over a complete V2 snapshot MAY provide complete search semantics. Search
over only a lazy Explorer cache MUST identify that it is limited to inspected
and cached paths, or report that complete search is unavailable. It MUST NOT
silently present a partial search as a complete corpus search.

## Failure containment

Failures are local to the narrowest affected prefix whenever possible. For
example:

```text
networking                 works
services                   works
crystal-forge.stig         works
crystal-forge.stig.active  unavailable
```

One bad branch MUST NOT destroy healthy siblings. The Explorer MUST NOT
fabricate children below an unreadable prefix. A root failure MAY prevent
browsing when no trustworthy root hierarchy can be established.

## Resource scheduling

The priority relationship is:

```text
authoritative primary/policy Nix work
       >
interactive Config exploration
       >
optional background full Config inventory
```

Config exploration MUST NOT delay deployment authority unnecessarily. The
historical failure mode was a job reported as `claimed` or `running` while it
indefinitely waited for `pg_advisory_xact_lock`. The required state model is:

```text
queued/waiting_for_capacity
          -> resource available
          -> running
```

The system MUST NOT hold a long-lived database transaction merely to wait for
evaluator capacity.

## Lifecycle and progress

Conceptual request states are:

```text
queued
waiting_for_capacity
running / inspecting
succeeded
failed
```

Useful sub-phases include `inspecting_tree`, `inspecting_option`,
`inspecting_provenance`, and `finalizing`. A request marked `running` MUST
have a live execution heartbeat. Waiting for capacity is not running. The UI
MUST NOT display a fabricated percentage when the total corpus is unknown.

## Security model

Config inspection follows these security rules:

- Browsers MUST NOT provide arbitrary Nix source or expressions.
- Path components are structured, validated, and bounded.
- Path depth, output size, and result counts are bounded.
- Existing authorization applies to every exact target and observation.
- POST mutations require CSRF protection.
- Credentials remain server-side.
- Exact revision inspection is read-only.
- Config inspection MUST NOT mutate `flake.lock`; use `--no-write-lock-file`
  or an equivalent read-only mode where appropriate.
- Secret values and traces MUST follow the existing redaction rules before
  persistence, indexing, comparison, logging, or API serialization.

## Process and timeout model

Every evaluator subprocess MUST be bounded by a timeout and an output limit.
Timeout and cancellation handling MUST clean up the complete process tree,
reap child processes, and prevent escaped `nix-eval-jobs` workers. A timeout
affects only the scoped Explorer request or prefix. Previously cached,
unrelated observations remain valid.

## Optional complete inventory

A complete inventory MAY be explicitly requested, generated as low-priority
background enrichment, or reused when already available. It supports Changed,
Drift, complete search, corpus statistics, and complete provenance comparison.
It is not required to open or browse Config.

Successful primary evaluation MUST NOT automatically schedule an expensive
full Config inventory. Primary success and Explorer enrichment are separate
decisions.

## API principles

The semantic operations are more stable than concrete route names:

- inspect root;
- inspect prefix;
- inspect an exact option;
- inspect provenance or detail;
- retrieve a cached observation; and
- request a complete inventory.

HTTP handlers MUST NOT synchronously perform expensive Nix work. They resolve
authorization and exact target identity, return cached observations when
available, and enqueue or reuse bounded work when an observation is missing.
Every operation accepts structured path input only and preserves exact target
identity through the worker and cache.

## Performance expectations

The design targets are qualitative rather than brittle absolute limits:

- Shallow browsing should normally complete in seconds.
- Cached browsing should require no Nix process.
- Expanding one branch MUST NOT secretly force the whole configuration.
- Provenance MAY cost more than tree navigation.
- Large full-corpus work is explicit or background work.

## Data flow

### Interactive Explorer

```text
Browser
   |
   v
API/cache check
   |
   +-- cached -> return
   |
   +-- missing -> enqueue scoped request
                         |
                         v
                    worker/Nix
                         |
                         v
                     cache result
                         |
                         v
                       Browser
```

### Authoritative separation

```text
configuration
      |
      +--> policy evaluator --> deployment gate
      |
      +--> Config Explorer --> human inspection
```

There MUST be no arrow from the Config Explorer cache into the deployment gate.

## Non-goals

Config Explorer does not:

- replace policy evaluation;
- become a generic arbitrary Nix evaluator;
- require a persistent interactive Nix REPL;
- guarantee a complete inventory merely because a user opened the tab;
- authorize Changed or Drift claims from lazy observations; or
- redesign every heavy-Nix workload scheduler in TASK-440.

## Future evolution

The following are possible future improvements, not current authority or
requirements:

- a warm internal evaluator/session if it is proven safe;
- smarter subtree prefetch;
- complete-inventory background scheduling;
- richer comparison after complete V2 artifacts exist;
- more precise progress reporting;
- cache retention and garbage-collection policies; and
- separate bounded interactive Nix concurrency if measurements prove it safe.

Any change that weakens the authority boundaries in this document requires an
architecture decision update before implementation.

## Current implementation

The current TASK-440 implementation is distributed across these boundaries:

- Explorer service and worker: `packages/default/crates/cf-server/src/services/config_inspections.rs`,
  `packages/default/crates/cf-server/src/bin/config-inspector-worker.rs`, and
  `packages/default/crates/cf-server/src/models/config_inspector.rs`.
- Trusted inspector expressions:
  `packages/default/crates/cf-server/src/models/config_inspector.nix`.
- Explorer queries and persistence:
  `packages/default/crates/cf-server/src/queries/config_inspections.rs` and
  `packages/default/crates/cf-server/src/security/snapshot_redaction.rs`.
- Evaluation snapshot queries and V2 model:
  `packages/default/crates/cf-server/src/queries/evaluation_snapshots.rs`,
  `packages/default/crates/cf-server/src/models/evaluation_snapshots.rs`, and
  `packages/default/crates/cf-server/src/models/config_snapshot_artifact.rs`.
- Snapshot and inspection schema:
  `packages/default/crates/cf-server/migrations/0245_evaluation_and_flake_output_snapshots.sql`,
  `0248_immutable_evaluation_artifacts.sql`,
  `0249_snapshot_capture_diagnostics.sql`,
  `0250_config_snapshot_artifact_v2.sql`,
  `0252_config_inspection_jobs.sql`,
  `0253_config_inspection_execution_ownership.sql`, and
  `0254_partial_config_option_inventories.sql`.
- API handlers and models: the Config inspection handlers and API models under
  `packages/default/crates/cf-server/src/handlers/api/` and
  `packages/default/crates/cf-server/src/api/models.rs`.
- Web UI Config view: the Config/system-detail surfaces under
  `packages/web-ui/src/views/` and `packages/web-ui/src/components/`.
- Deliberately separate policy evaluator path:
  `packages/default/crates/cf-server/src/models/evaluate_with_policies.rs`,
  `packages/default/crates/cf-server/src/deployment/mod.rs`, and the policy
  services under `packages/default/crates/cf-server/src/services/`.

These paths identify ownership. They do not authorize a future change to make
Explorer data authoritative for policy or deployment.

## Decision record

**Decision 1:** Config Explorer is observational, not deployment authority.  
**Decision 2:** Policy enforcement continues to evaluate policies independently.  
**Decision 3:** Config browsing is lazy and path-scoped.  
**Decision 4:** Complete V2 snapshots are optional enrichment and comparison artifacts.  
**Decision 5:** Partial or unreadable branches remain local failures.  
**Decision 6:** Waiting for evaluator capacity is distinct from running inspection.  
**Decision 7:** Explorer results are cached against immutable exact target identity.  
**Decision 8:** Arbitrary Nix expressions are never accepted from clients.  
**Decision 9:** Exact revision inspection is read-only.  
**Decision 10:** Cached or partial Explorer data cannot produce authoritative Changed or Drift conclusions.

Future work that violates one of these decisions MUST amend this document as
an explicit architecture change instead of silently changing the behavior.
