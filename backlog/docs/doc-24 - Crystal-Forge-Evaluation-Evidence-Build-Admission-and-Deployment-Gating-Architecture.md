---
id: doc-24
title: >-
  Crystal Forge Evaluation, Evidence, Build Admission, and Deployment Gating
  Architecture
type: specification
created_date: '2026-09-06 23:06'
tags:
  - architecture
  - evaluation
  - policy
  - build-admission
  - deployment
  - post-task-440
---
**Status:** Design proposal / post-TASK-440 follow-up
**Intended timing:** Implementation tasks are sprint-ready in `To Do`, each depending on TASK-440, and must not start until TASK-440 is complete and proven in dev.
**Primary objective:** Preserve Crystal Forge's fast evaluation and drift-detection loop while eliminating duplicated configuration/policy evaluation and using build time as latency budget for deeper inspection.

---

## 1. Problem statement

Crystal Forge currently has several overlapping mechanisms that answer related questions about a NixOS configuration:

- the primary `nix-eval-jobs` evaluator discovers system derivations and emits policy-related metadata;
- deployment policies independently evaluate configuration facts such as `cfAgentEnabled`, required packages, and arbitrary config expressions;
- TASK-440 adds a targeted Config Inspector that evaluates effective options, values, provenance, overridden definitions, source identity, and other semantic configuration evidence;
- Flake Explorer separately evaluates flake-level exports and metadata;
- deployment/build eligibility is currently represented partly by derived fields such as `cf_agent_enabled`, `policy_requirements_met`, and `policy_results`.

This creates duplicate evaluation work and multiple partially-overlapping sources of truth.

The desired future architecture is **not** one giant evaluator. Instead, Crystal Forge should have distinct evaluation tiers with explicit latency and authority boundaries:

1. **Fast identity evaluation** — fleet-wide and latency-critical.
2. **Deep per-configuration inspection** — rich semantic evidence, isolated per configuration, parallel with builds.
3. **Evidence-driven policy assessment** — mostly Rust/DB evaluation over immutable artifacts.
4. **Build admission and prioritization** — progressively stronger as evidence arrives.
5. **Deployment gating** — strict final decision using all required evidence.

---

## 2. User intent / product behavior

The intended operational flow is:

1. Evaluate configurations **fast** so Crystal Forge quickly knows what each system configuration should produce.
2. As soon as the expected NixOS derivation/store identity is known, compare it to the system's latest reported running store path and classify drift immediately.
3. As soon as Crystal Forge can cheaply prove that the Crystal Forge agent is enabled, admit or prioritize the build according to the selected build-admission mode.
4. Start the full targeted Config inspection at the same time as the build.
5. Complete rich option/config evaluation and policy assessment **during build time**, ideally before the build/cache work finishes.
6. Use later policy results to promote, hold, deprioritize, or gate not-yet-started work.
7. Always require the configured deployment policy/evidence gates before deployment, even when build admission is permissive.
8. Provide a UI option to build everything immediately when the operator prefers throughput/latency over compute conservation.

The design goal is therefore:

```text
fast eval -> drift known + candidate build admitted
                |                     |
                |                     +------> build/cache work
                |
                +------> deep Config inspection -> policy assessment

Deployment happens when BOTH:
  build/cache readiness
  AND required policy/evidence readiness
are satisfied.
```

---

## 3. Core architectural principle: preserve the narrow fast evaluator

The narrow evaluator boundary established during TASK-440 is intentional architecture, not temporary debt.

The primary evaluator must remain cheap and safe enough to run fleet-wide without forcing unrelated lazy flake content or deep module-system exploration.

### Fast evaluator responsibilities

The fast evaluator should produce only facts needed to establish build identity, drift identity, and the earliest build-admission decision:

- exact flake/revision identity;
- configuration name;
- `config.system.build.toplevel` / derivation identity;
- expected NixOS output/store identity when available through the normal build target;
- `cfAgentEnabled` or equivalent narrowly-whitelisted agent-capability fact;
- evaluator success/failure necessary to establish that the configuration is buildable.

### Fast evaluator non-responsibilities

It must **not** grow back into a generic configuration-inspection or arbitrary-policy evaluator.

Do not put these back into the primary bulk evaluator:

- `cfg.options` traversal;
- `_module.graph` exploration;
- arbitrary option-tree introspection;
- full provenance extraction;
- discarded-definition analysis;
- general custom policy expressions;
- Flake Explorer/module enumeration;
- rich Config UI evidence.

The fast lane may gain additional facts only when they are demonstrably:

- cheap;
- universally useful;
- safe for bulk evaluation;
- necessary for early scheduling/admission decisions.

---

## 4. Three-tier evaluation model

### Tier 1 — Fast identity evaluation

Purpose:

> What exact system should this configuration produce, and can it enter the build pipeline yet?

Characteristics:

- fleet-wide;
- lowest latency;
- narrow Nix expression;
- failure-isolated by ordinary `nixosConfigurations` semantics;
- establishes expected derivation/store identity;
- immediately enables drift classification;
- may establish `cfAgentEnabled` for early build admission.

### Tier 2 — Deep Config inspection

Purpose:

> What exactly is inside this one configuration, and why?

Characteristics:

- one selected configuration per inspection job/process;
- starts as soon as Tier 1 establishes the exact config identity;
- runs concurrently with build/cache work;
- produces `ConfigArtifactV2` / later compatible versions;
- redacted before persistence;
- immutable and content-addressed;
- complete enough to become the canonical configuration-fact evidence source.

Deep inspection includes, as available:

- exact structured option paths;
- effective values;
- option types and declarations;
- source/module provenance;
- active/surviving definitions;
- priority-discarded definitions;
- merge order;
- definition values;
- global provenance state;
- exact target/source/carrier identity;
- comparison readiness.

### Tier 3 — Evidence/policy evaluation

Purpose:

> Given the immutable evidence available for this exact configuration, is this artifact eligible to build and/or deploy?

Characteristics:

- primarily Rust/SQL, not repeated Nix evaluation;
- consumes exact evidence artifacts;
- produces versioned policy assessments;
- separates build admission from deployment authorization;
- supports progressive decisions as evidence arrives.

---

## 5. Parallel pipeline

The target execution graph is:

```text
                            COMMIT / REVISION
                                  |
                                  v
                         FAST IDENTITY EVAL
                                  |
                    +-------------+-------------+
                    |                           |
                    v                           v
             DRIFT CLASSIFICATION          BUILD ADMISSION
             expected vs running               |
                                                v
                                             BUILD
                                                |
                                                v
                                              CACHE

                    +---------------------------+
                    |
                    v
             DEEP CONFIG INSPECTION
                    |
                    v
             CONFIG EVIDENCE READY
                    |
                    v
              POLICY ASSESSMENT
                    |
                    +---------------------------+
                                                |
                                                v
                                      FINAL DEPLOYMENT GATE
```

The critical-path target is:

```text
time_to_deployment =
    fast_eval
    + max(
        build_and_cache,
        required_config_inspection_and_policy_assessment
      )
    + deployment_runtime_gates
```

It should **not** become:

```text
fast_eval
+ deep_inspection
+ policy_eval
+ build
+ cache
+ deployment
```

The `max(...)` behavior is a core performance requirement.

---

## 6. Drift detection must stay fast

The first useful answer Crystal Forge should provide after a commit arrives is whether each system's running configuration matches the expected selected configuration.

Conceptually:

```text
running_store_path == expected_selected_store_path
    -> matches selected configuration

running_store_path != expected_selected_store_path
    -> drifted / behind / different configuration
```

This classification must depend only on Tier 1 identity facts and persisted agent-reported state.

It must **not** wait for:

- full Config inspection;
- provenance enrichment;
- Stage-2 definition values;
- policy assessment;
- build completion.

### Performance objective

**Time to expected system identity and drift classification must be no worse than the current fast evaluation loop.**

---

## 7. ConfigArtifactV2 becomes canonical configuration evidence

After TASK-440 is complete, the deep Config artifact should become the canonical source for semantic configuration facts.

The policy engine should increasingly consume this evidence rather than independently asking Nix the same questions.

### Candidate policy mappings

| Policy / assertion | Future evidence source |
|---|---|
| `nixos_option` assertion | Config artifact |
| `require_cf_agent` | Fast fact initially; Config artifact for canonical evidence |
| `require_packages` over `environment.systemPackages` | Config artifact |
| typed config composites | Config artifact |
| simple `config.foo.bar == literal` custom rules | Config artifact |
| safe typed package/config assertions | Config artifact |
| arbitrary `custom_check` Nix expression | isolated/sandboxed Nix evaluator escape hatch |
| CVE thresholds / CVE requirements | CVE scan artifact |
| build-closure package evidence | build/closure artifact |
| approvals | approval state |
| time windows | deployment-time state |
| canary rollout | fleet/deployment-time state |
| runtime/environment conditions | runtime evidence |

### Important distinction

The policy engine should become an **evidence consumer**, not another broad evaluator.

Arbitrary Nix evaluation remains available only as an explicit escape hatch for policy logic that cannot be represented against typed evidence.

---

## 8. Progressive Config evidence

Rich Config inspection contains more information than most policies require.

Do not require complete provenance enrichment before a simple policy can run.

Internally, think in progressive evidence readiness:

### Stage A — policy-ready configuration facts

Examples:

- effective option values;
- relevant option metadata;
- package/config facts;
- exact artifact/config identity;
- enough information for typed config assertions.

Once Stage A is ready, config-derived policy assessment may begin.

### Stage B — rich inspection/audit enrichment

Examples:

- complete raw definitions;
- priority-discarded definitions;
- merge ordering;
- per-definition contributed values;
- source/module provenance;
- rich UI/audit explanation.

Stage B should enrich the Config UI and audit trail, but should not delay build or deployment unless a particular policy explicitly declares that it depends on Stage-B evidence.

---

## 9. Build admission is separate from deployment authorization

Crystal Forge should distinguish:

- **Can/should we spend compute building this derivation?**
- **May this completed artifact be deployed?**

These are different policy questions.

A build may be safe to execute even when it is not yet eligible for deployment.

### Default behavior for already-running builds

If later evidence shows a policy failure:

- do not automatically kill an already-running build by default;
- prevent or hold queued/not-yet-started work as configured;
- deprioritize speculative work when appropriate;
- always block deployment when a strict deployment policy fails.

Cancellation may become an explicit scheduler/resource policy later, but should not be the default semantic consequence of a deployment-policy failure.

---

## 10. Build admission modes

The UI should eventually expose an explicit build-admission mode.

### Mode A — Build all

Purpose: maximize throughput / minimize latency.

Behavior:

- as soon as Tier 1 produces a valid derivation, the build is eligible;
- config-derived policy checks do not block build start;
- deep Config inspection still runs in parallel;
- policy still gates deployment;
- later policy results may affect deployment but not build admission.

### Mode B — CF-enabled

Recommended default candidate.

Behavior:

- Tier 1 must prove `cfAgentEnabled = true` before the build is admitted;
- deep policy evaluation runs concurrently with the build;
- pending builds may be held/deprioritized if richer evidence later fails;
- policy-passed work may be promoted in scheduler priority;
- deployment still requires all configured deployment gates.

This avoids obviously useless builds for configurations that cannot run the Crystal Forge agent while preserving the fast build pipeline.

### Mode C — Policy-gated

Purpose: conserve build resources aggressively.

Behavior:

- do not start the build until all configured **pre-build** policies have sufficient evidence and pass;
- deployment policies may be a superset of pre-build policies;
- missing evidence is an explicit pending/held state, not an implicit pass.

---

## 11. Progressive build-admission state machine

Build eligibility should be represented by explicit states/reasons rather than one opaque boolean.

Conceptual progression:

```text
Evaluated
   |
   v
Candidate
   |
   v
CF-capable
   |
   v
Policy-qualified
   |
   v
Build-ready / Build-running
   |
   v
Deployment-qualified
```

Possible decision reasons:

```text
held: evaluator_failed
held: cf_agent_disabled
held: config_evidence_pending
held: strict_prebuild_policy_failed
eligible: build_all_override
eligible: cf_agent_verified
eligible: policy_verified
priority: speculative
priority: policy_verified
blocked_deploy: strict_config_policy_failed
blocked_deploy: cve_policy_failed
blocked_deploy: approvals_missing
```

These reasons should be persisted/auditable and visible in the UI.

---

## 12. Build prioritization

Admission and priority are separate dimensions.

A scheduler may admit several candidates but prefer work backed by stronger evidence.

Example priority model:

```text
highest priority:
  build already needed by explicit deployment request
  + config evidence ready
  + policies pass

high priority:
  auto-latest target
  + config policies pass

normal priority:
  cfAgentEnabled verified
  + deep Config inspection pending

low/speculative priority:
  build-all mode
  + no policy evidence yet

held:
  known strict pre-build policy failure
  or selected mode requires missing evidence
```

The exact scheduler values are implementation details. The design requirement is that stronger evidence can **promote** a build and policy failure can **hold/deprioritize** future work without requiring the build farm to wait for all deep inspection before doing anything.

---

## 13. PolicyAssessment becomes the decision source of truth

Today fields such as:

- `derivations.cf_agent_enabled`;
- `derivations.policy_requirements_met`;
- `derivations.policy_results`;

carry overlapping policy/evaluator meaning.

The future source of truth should instead be reproducible from:

```text
immutable evidence artifacts
        +
exact policy-set/version
        |
        v
PolicyAssessment
        |
        +--> BuildAdmissionDecision
        |
        +--> DeploymentDecision
```

`policy_requirements_met` may remain temporarily as a materialized compatibility/cache field during migration, but it should not remain the authoritative irreducible fact.

A deployment decision should eventually be explainable as:

> Deployment X was allowed because Config artifact A, CVE artifact B, build/closure artifact C, policy-set version D, and approval/runtime evidence E produced PolicyAssessment F.

This provides deterministic auditability and removes hidden evaluator side effects.

---

## 14. Policy phases

Policies should explicitly declare when they apply.

At minimum, support conceptual phases such as:

### Pre-build

Used to conserve compute.

Examples:

- CF agent must be enabled;
- selected high-confidence config requirements;
- organization-specific rules that intentionally prevent build spend.

### Build-priority

Used to prioritize rather than block.

Examples:

- policy evidence complete;
- deployment likely/explicitly requested;
- target is currently drifted/behind;
- higher environment priority.

### Pre-deploy

Strict final authorization.

Examples:

- config policies;
- CVE gates;
- approval requirements;
- canary policy;
- time windows;
- runtime/fleet state;
- exact artifact/evidence lineage.

A policy may apply to more than one phase, but the phase must be explicit.

---

## 15. Flake Explorer relationship

Flake Explorer should share the same **artifact/job infrastructure**, but it should not be collapsed into the per-configuration Config artifact.

These answer different questions:

### Config artifact

> What does `nixosConfigurations.<name>` mean at this exact revision?

### Flake artifact

> What does this revision export, what inputs/modules does it expose, and what is the flake-level structure?

Long-term artifact family may look like:

```text
EvaluationArtifact
├── ConfigArtifact
├── FlakeOutputArtifact
├── CveArtifact
├── BuildClosureArtifact
└── PolicyAssessmentArtifact
```

They should share infrastructure where sensible:

- exact revision identity;
- immutable lifecycle;
- content addressing;
- redaction;
- certification;
- current selectors;
- retention;
- bounded DB-only readers;
- execution/job orchestration;
- cancellation/resource limits;
- audit identity.

But they remain separate semantic artifact types.

---

## 16. Isolation requirements

Deep inspection must remain per-configuration.

Do not reintroduce a whole-flake semantic inspection that forces all sibling configurations/modules merely to inspect one target.

Desired behavior:

```text
flake revision
├── config A -> targeted inspection A
├── config B -> targeted inspection B
├── config C -> targeted inspection C
└── broken config D -> failure isolated to D
```

One broken/lazy/poisoned configuration or unrelated exported module must not contaminate another configuration's fast evaluation or inspection.

---

## 17. Performance requirements / SLOs

The follow-up implementation should make performance an explicit acceptance criterion.

### Required performance goals

1. **Time to expected system identity**
   Must be no slower than the current fast evaluation loop.

2. **Time to drift classification**
   Immediate after Tier-1 identity is available. Must never wait for deep Config inspection or build completion.

3. **Time to initial build admission**
   In Build-all mode: immediately after successful Tier-1 derivation discovery.
   In CF-enabled mode: immediately after Tier 1 proves `cfAgentEnabled=true`.

4. **Deep Config start**
   Starts as soon as the exact per-config Tier-1 identity is established.

5. **Parallelism**
   Build/cache work and deep Config inspection must overlap.

6. **Policy assessment**
   Runs as soon as sufficient evidence exists; should not wait for rich provenance that the policy does not require.

7. **Normal-case completion**
   Required Config inspection + policy assessment should normally finish before build/cache work finishes.

8. **Deployment**
   May proceed as soon as both the artifact pipeline and all required deployment evidence are ready.

9. **Rich provenance**
   Must not delay build/deployment unless explicitly required by policy.

10. **No bulk-evaluator regression**
    Deep config/policy requirements must never expand the primary evaluator's search space back into `cfg.options`, `_module.graph`, `flake.nixosModules`, `lib.evalModules`, or similar broad exploration.

---

## 18. Example timing

A healthy common-case pipeline should resemble:

```text
T+0s      revision discovered

T+1-2s    fast evaluation complete
           - drvPath / expected store identity known
           - drift classification known
           - cfAgentEnabled known
           - eligible builds admitted according to mode

T+2s      build starts
           deep Config inspection starts

T+5-15s   policy-ready Config evidence complete
           config policies assessed
           queued builds promoted/held/deprioritized

T+10-20s  rich Config provenance completes
           UI/audit enrichment available

T+30-120s build/cache finishes

           required policy evidence is already ready

           deployment proceeds immediately if authorized
```

With **Build all** enabled:

```text
fast eval -> build immediately
```

without waiting for any build-admission policy, while deployment remains policy-gated.

---

## 19. Failure semantics

### Fast evaluation failure

- no valid derivation identity;
- build cannot start;
- drift target for that revision/config is unavailable;
- deep inspection may be skipped or report an explicit evaluation failure depending on job architecture.

### CF agent disabled

In CF-enabled or stricter admission mode:

- build held by default;
- explicit reason visible;
- Build-all mode may override build admission;
- deployment remains subject to configured policy.

### Deep Config inspection unavailable

- do not fabricate pass/fail configuration evidence;
- config policies that require missing evidence remain pending/unavailable according to policy semantics;
- Build-all may continue building;
- CF-enabled mode may continue already-admitted builds;
- deployment fails closed when required evidence is unavailable.

### Policy failure arrives after build starts

Default:

- running build may finish;
- queued/not-started related work may be held/deprioritized;
- deployment is blocked if the failed policy is strict for deployment.

### Rich provenance Stage-B failure

- Stage-A policy facts remain usable if independently certified;
- UI shows provenance enrichment unavailable;
- deployment is unaffected unless a policy explicitly depends on Stage-B evidence.

---

## 20. Security and integrity requirements

- All semantic Config evidence is redacted before persistence, digesting, indexing, or API exposure.
- Build/deployment decisions bind to exact immutable artifact identities and exact policy-set/version identities.
- Missing or unsupported evidence is not silently converted to success.
- Arbitrary Nix policy execution remains isolated and explicitly classified as such.
- Store-path equality alone must not be used as historical provenance proof.
- Historical generation/deployment evidence must retain exact lineage.
- The fast evaluator must not be widened merely to make a policy convenient.

---

## 21. Migration strategy after TASK-440

Do **not** perform this consolidation inside TASK-440 / MR !323.

TASK-440 should first finish:

- V2 selector isolation;
- V2 DB-only reader;
- production targeted Config Inspector runner/job;
- API lifecycle/action;
- first backend dev deployment;
- UI wiring;
- historical semantics and final verification.

Then create follow-up tasks.

### Follow-up Task 1 — Canonical evaluation evidence architecture

Inventory every current evaluation path that reads or evaluates configuration facts.

Classify each into:

- fast derivation/identity fact;
- Config artifact fact;
- build/closure fact;
- CVE fact;
- runtime/deployment fact;
- arbitrary Nix escape-hatch fact.

Define:

- evidence identity/version contract;
- `PolicyAssessment` model;
- build-admission decision model;
- deployment-decision model;
- policy phase model;
- migration compatibility plan for existing derivation policy fields.

### Follow-up Task 2 — Move config-derived policies to Config artifacts

Migrate suitable policy types, starting with:

- `require_cf_agent` canonical evidence;
- `require_packages` / package-set config evidence;
- typed `nixos_option` assertions;
- representable custom config assertions;
- compatible composite config assertions.

Preserve arbitrary Nix only where typed evidence cannot express the policy.

### Follow-up Task 3 — Progressive build admission and scheduler priority

Implement:

- Build-all mode;
- CF-enabled mode;
- Policy-gated mode;
- explicit build admission reasons;
- evidence-aware priority promotion/holding;
- UI configuration and status display;
- concurrent build + Config inspection scheduling.

### Follow-up Task 4 — Reduce primary evaluator

After policy consumers are migrated:

- remove duplicate config-policy evaluation from the bulk evaluator;
- keep only fast identity/build facts and the smallest necessary early-admission facts;
- verify eval-to-drift/build latency does not regress.

### Follow-up Task 5 — Unify artifact/job infrastructure

Refactor Config Inspector, Flake Explorer, and other immutable evidence producers onto common execution/lifecycle infrastructure where appropriate without merging their semantic data models.

---

## 22. Acceptance criteria for the consolidation initiative

The initiative is successful when all of the following are true:

- [ ] Fast evaluation remains narrow and fleet-safe.
- [ ] Expected config store/derivation identity is available as quickly as or faster than today.
- [ ] Drift classification does not wait for deep inspection.
- [ ] Deep Config inspection runs concurrently with builds.
- [ ] Config-derived policies consume canonical Config artifact evidence instead of re-evaluating equivalent Nix facts.
- [ ] Arbitrary custom Nix remains an explicit isolated escape hatch rather than the default policy mechanism.
- [ ] Build admission supports Build-all, CF-enabled, and Policy-gated modes.
- [ ] Scheduler priority can improve as stronger policy evidence arrives.
- [ ] Known policy failure can hold/deprioritize not-yet-started work.
- [ ] Running builds are not automatically cancelled solely because a deployment policy later fails unless explicitly configured.
- [ ] Deployment remains fail-closed on required missing/failed evidence.
- [ ] Policy decisions bind to exact immutable evidence and exact policy-set versions.
- [ ] Existing `policy_requirements_met`-style booleans are no longer the sole source of policy truth.
- [ ] Normal-case policy-ready Config inspection finishes before build/cache work completes.
- [ ] Rich provenance does not unnecessarily extend the deployment critical path.
- [ ] Flake Explorer shares infrastructure but remains semantically separate from Config artifacts.
- [ ] A broken sibling configuration cannot contaminate another configuration's inspection.
- [ ] No regression reintroduces broad option/module exploration into the primary evaluator.

---

## 23. Non-goals

This design does not require:

- one universal artifact schema for every evidence type;
- one giant evaluator process;
- making Flake Explorer a view over Config artifacts;
- cancelling all builds that later become undeployable;
- forcing every policy to be a pre-build gate;
- eliminating arbitrary Nix policy expressions immediately;
- changing TASK-440 scope before TASK-440 is complete.

---

## 24. Design invariants to carry into future tasks

1. **Fast identity first.**
2. **Drift must be known from the fast path.**
3. **Deep inspection hides behind build time.**
4. **The primary evaluator stays narrow.**
5. **ConfigArtifact is canonical configuration evidence.**
6. **Policies consume evidence; they do not duplicate evaluation by default.**
7. **Build admission and deployment authorization are separate.**
8. **Operators can choose compute conservation vs build-everything throughput.**
9. **Evidence readiness is progressive.**
10. **Policy and scheduler state must have explicit reasons, not opaque booleans.**
11. **Deployment fails closed on required unavailable evidence.**
12. **Artifact/evidence identity is immutable and auditable.**
13. **Per-config failure isolation is mandatory.**
14. **Performance preservation is an acceptance criterion, not an assumption.**

---

## 25. Short version

The future Crystal Forge evaluation pipeline should be:

```text
FAST EVAL
  -> exact config/build identity
  -> expected store path
  -> instant drift classification
  -> cfAgentEnabled
  -> early build admission

IN PARALLEL
  BUILD/CACHE
  DEEP CONFIG INSPECTION

CONFIG EVIDENCE
  -> typed policy assessment
  -> promote / hold / deprioritize queued work

FINAL DEPLOYMENT
  requires build readiness
  + exact Config evidence
  + policy assessment
  + CVE / approval / runtime gates
```

The result should use compute more intelligently **without sacrificing the fast eval loop** and should eliminate duplicated config-policy evaluation by making immutable Config evidence the canonical semantic input to policy decisions.
