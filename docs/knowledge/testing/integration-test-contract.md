---
type: Testing Guide
title: Crystal Forge integration test maintenance contract
description: Defines the four integration check owners and the rules for maintaining their coverage when approved product requirements, UI designs, or implementation change.
tags:
  - crystal-forge
  - testing
  - integration
  - design
implementation_status: partial
generated:
  by: anthropic/claude-opus-5-5
  at: 2026-10-17T11:40:00-05:00
---

# Crystal Forge integration test maintenance contract

**Status:** Adopted by the owner on 2026-10-17 through TASK-490. The maintenance
rules below are in force. `implementation_status: partial` describes coverage,
not the rules. The harness exists under `lib/integration-test/`; Access and
Delivery are registered and run automatically in GitLab. Access's
local-bootstrap scenario passed all 23 cases through the CI job script at
`c254b1c54e0f40adfaa9d05b5766cfeffae9bb89`: the original five bootstrap,
five session and six Operator/Viewer authorization cases, then seven real
Admin-form cases with reload and independent SQL observations. The VM
executed newly. Earlier ten- and sixteen-case results remain
in the execution history. This proves the selected subset, not complete Access,
Delivery, Assurance, Upgrade, or the overhaul. TASK-490.25/.26 startup repairs
remain integrated; unrelated TASK-492 is separate. TASK-490.1 and the repairs
are in `Review`; TASK-490.2, TASK-490.3 and TASK-490.24 remain `In Progress`. Exact
environment scope and registration policy still await their own decisions.
The owner now authorizes legacy source retirement before replacement coverage
is complete. The former optional-manual coexistence policy is superseded.
The [coverage map](integration-coverage-map.md) preserves receipts, reviewer
artifacts, execution history, packet groupings, source dispositions and gaps.
Documentation-only changes do not require another VM run.

**Source baseline:** Crystal Forge `origin/dev` at `5ae7feab8a7b372e8efe0d2dd21b6f53f0625064` after TASK-488 merged. Earlier execution records identify their pre-rebase base where relevant. Source links appear at the end of this document. The existing check implementations were not used to choose the four-check structure.

**Adoption record:** The owner-supplied proposal was adopted through TASK-490. The owner's corrected layout uses native Snowfall checks, explicit scenario registration, and shared helpers under `lib/integration-test/`. Discoverability does not imply complete coverage. The CI transition policy in the transition rule below applies.

The words **MUST**, **MUST NOT**, and **MAY** define the maintenance rules. An explicit owner decision in the active task can authorize a change to this contract; agents must cite that decision rather than ask for it again.

## 1. Fixed check inventory

Crystal Forge has four integration check owners. Browser tests run inside the owner of the workflow they exercise.

| Flake check | Contract ownership | Representative proof |
| --- | --- | --- |
| `integration-access` | Fresh installation, bootstrap, authentication, sessions, role and environment authorization, administration, resource configuration, profile preferences, notifications, and common application-shell behavior. | A real browser action reaches the server, produces its authorized result, and survives a subsequent read. Denied actions have no protected side effect. |
| `integration-delivery` | Git ingestion, exact evaluation, Config Inspector, build scheduling and execution, builder transports, cache publication and retrieval, agent delivery, activation, rollback, and operational recovery. | The selected source produces the intended output; the intended target actually runs it; CF reports the corresponding identities and states. |
| `integration-assurance` | CVE scanning, evidence authority, policy evaluation and enforcement, compliance, STIG controls, hardening, triage, waivers, POA&M, and assurance imports/exports. | Real inputs produce independently expected evidence and decisions. UI, stored records, enforcement, and exported records agree on identity and scope. |
| `integration-upgrade` | The supported upgrade path, migrations, persisted identities and history, recovery of migrated or in-flight state across the version transition, and the explicitly supported agent/builder compatibility window. | A real supported older version creates data; the new version migrates and uses it, then completes a representative new operation. |

The final `checks.x86_64-linux` inventory is exactly four TOTAL attributes, with
the names in the table above. Lower-level verification and developer tools
belong outside `checks`, in package or app outputs. The number of assertions is
not fixed. New supported behavior can require more assertions.

TASK-490.24 source-retirement decision (2026-10-18) supersedes the earlier
retain-until-four-pass policy and the optional-manual coexistence policy.
The nine legacy suites are retired with owner approval before replacement
coverage is complete. The initial seventeen-definition package relocation is
superseded: all fifteen public `verify-*` wrappers are removed. Unique assertions
run in ordinary component tests, direct CI commands and existing isolated-DB
workflows. Documentation and screenshot tools remain ordinary packages.
The current inventory is Access and Delivery (two versus 27), with selected
bootstrap, session, role-authorization and Admin-form persistence coverage.
The earlier 16-case subset remains first and unchanged; seven Admin-form cases
extend the recorded Access run to 23 cases. Delivery contains the source,
native evaluation, API-builder and target-agent lifecycle. Its exact execution
record is in the coverage map. Assurance and Upgrade are absent.
This is not final four-owner completion. No placeholder owner is permitted.
The earlier optional-manual CI transition remains in the execution history;
it does not describe the current source-retirement policy.

Final inventory verification evaluates the complete attribute set:

```bash
nix eval --json .#checks.x86_64-linux --apply builtins.attrNames
nix eval --json .#checks.x86_64-linux --apply 'c: builtins.attrNames c == [ "integration-access" "integration-assurance" "integration-delivery" "integration-upgrade" ]'
```

Final acceptance requires the predicate to return `true`, all four real owner
aggregates to pass, and complete accepted coverage. Attribute count alone does
not establish execution, completeness, or absence of duplicate work.

An agent MUST use the smallest change that proves the affected contract, in this order:

1. Retain an existing scenario when it already proves the requirement.
2. Update or extend that scenario.
3. Add a named scenario inside the same check when a distinct starting state, interaction, failure, or integration boundary requires it.
4. Propose a change to the top-level inventory only under Section 9.

A new page, component, API route, cache adapter, policy kind, task, or regression does not by itself justify a new flake check. Splitting a test file for clarity does not require a new check. Focused local execution also does not require a new check.

Each check MUST run independently. Checks MAY share immutable Nix inputs, fixture definitions, and test helpers. They MUST NOT depend on another check's database, VM, session, or previously produced runtime output.

**Harness location and registration.** Shared helpers live in
`lib/integration-test/`. Native owner checks call
`lib.crystal-forge.makeIntegrationTestHarness { inherit pkgs inputs; }` and
explicitly register `scenarios` in their own `default.nix`. The normal
`pkgs.linkFarm` aggregate builds every registered scenario gate. Access and
Delivery are currently discoverable. Compatible behavior modules use the
existing Nix, Python and browser-script composition inside a shared lifecycle;
tracking tasks do not define files or derivations. Focused execution selects
`checks.x86_64-linux.<owner>.scenarios.<scenario>`; `.run` is evidence only.
Meaningful WIP owners may be discoverable without a completeness flag. Keep
scenario files and assets under their owner; do not add placeholders or custom
discovery. The [coverage map](integration-coverage-map.md#harness-interface)
records actual interfaces, behavior-oriented composition and remaining gaps.

Each scenario produced by `mkScenario` runs in its own fresh NixOS VM test and database. The normal owner check aggregates those independent scenario gates. Scenario files and immutable fixture definitions MAY be reused. Live databases, VMs, sessions, runtime identities, and successful outputs MUST NOT be reused between scenarios or owner checks. A Backlog task dependency names code/fixture prerequisites only; it does not make a producer scenario's runtime data available to a dependent scenario. Within one scenario, later cases MAY use identities and records created by earlier real workflows; the existing receipt mechanism carries generated identities. A scenario MUST create the real starting state it needs in its own run, without seeding the result under test.

## 2. Authority and the documents agents must read

The authoritative sources have different responsibilities.

| Source | Controls |
| --- | --- |
| The owner's accepted requirement or decision, and applicable approved domain specifications | Product meaning, authorization, identity, persistence, evidence authority, supported integrations, and compatibility. |
| The approved design implementation in `docs/design/CrystalForge/`, at an identified revision | UI structure, content hierarchy, presentation, static copy, and designed interactions/states. |
| This contract | Check ownership, coverage maintenance, fixture boundaries, verification, and evidence requirements. |
| Test code | The executable procedure that proves the accepted requirement. |

Production code and past test output show current behavior. They do not approve a requirement. A design's mock data does not define backend identity, authorization, lifecycle, or evidence rules.

Agents MUST distinguish approved requirements from proposed, historical, and as-built descriptions. A design file present in the repository is not sufficient evidence that every proposed behavior inside it is approved. Read the active task and the source document's status and scope.

Before changing browser-visible behavior, record the exact approved reference: commit or preserved artifact revision, path/component, and affected states. Compare against the preceding approved reference where one exists. If only an external example was supplied, preserve an identifiable reference before deriving expected results from it.

If an approved domain requirement has no corresponding design state, identify the file/component, condition, missing fact or interaction, and affected acceptance case. Mark that dependent UI case **Blocked: design gap**. Continue independent authorized work. Do not invent a UI element, edit the design to match the application, hide the fact, or present an unknown result as success. This follows CF's existing Systems design authority rule.

Keep this maintenance contract outside the design handoff directory. The design handoff can be replaced during a design update. Link to the approved reference rather than duplicating its complete contents here.

## 3. Minimal coverage catalog

Maintain one compact catalog. Each durable workflow has one primary owner. The catalog records what must be proved and links to its acceptance authority and executable scenario. Test code contains the detailed procedure.

The table below is the stable ownership map. The per-scenario catalog, authority links, status, and legacy retirement map are kept in the [integration coverage map](integration-coverage-map.md) so that this contract changes only when a rule or ownership changes. An identifier is not evidence that a matching test exists.

| Contract ID | Primary check | Required observable contract |
| --- | --- | --- |
| `access.lifecycle` | Access | Fresh bootstrap, local/OIDC sign-in and sign-out, session protection, and role/environment restrictions work through the deployed application and real API. |
| `access.configuration` | Access | Administration and resource forms persist authorized changes; shared save, cancel, loading, and error behavior works. Preferences and notifications produce their specified results. Provider execution belongs to Delivery; assurance assignment and evidence effects belong to Assurance. |
| `access.shell` | Access | Common navigation, route access, keyboard/focus mechanics, and shell preferences follow the approved design. |
| `delivery.evaluation` | Delivery | Git/event ingestion and evaluation preserve exact source and system identity; evaluation and Config views expose the correct records and navigation context. |
| `delivery.build` | Delivery | Supported builder transports, job assignment, cancellation, retry, and stale-worker handling produce the specified real execution result. |
| `delivery.publication` | Delivery | Each supported cache implementation proves its distinct publication/authentication boundary; verified destination and output identity govern subsequent retrieval and deployment. |
| `delivery.activation` | Delivery | Supported deployment modes, boot behavior, rollback, target isolation, drift, and recovery agree with the target's actual state. |
| `assurance.scanning` | Assurance | Scans run against the intended real closure and target; Current, historical, failed, missing, and clean evidence remain distinct. |
| `assurance.policy` | Assurance | Policy versions, assignments, scope, enforcement/report-only behavior, compliance mappings, STIG, and hardening produce the accepted evidence and admission decisions. |
| `assurance.remediation` | Assurance | Finding navigation, triage, waivers, and POA&M preserve identity, evidence authority, and accepted remediation continuity. |
| `assurance.interchange` | Assurance | Supported imports are validated and atomic; actual downloaded exports have valid structure and the expected records, identities, and scope. |
| `upgrade.continuity` | Upgrade | Supported older data and credentials survive the normal migration/startup path; migrated or in-flight state recovers across the version transition, and the declared client compatibility window remains usable. |

When implemented, add an **Authority** link and a **Scenario** file/name reference to each applicable row. Do not mark a row covered until that reference points to an executable case that proves it. Record accepted but unimplemented requirements as coverage gaps. A passing subset does not establish complete coverage of the accepted scope.

Use stable identifiers based on behavior, such as `delivery.evaluation.matrix`, rather than task numbers or temporary filenames. Do not add an identifier for every CSS property, button, or screenshot.

Active scenario and fragment filenames and ownership descriptions MUST name
stable behaviors, not implementation tasks. Task IDs MAY identify historical
authority or execution records. A file split MUST retain the smallest existing
composition mechanism and document runtime prerequisites in the composition and
module headers: required helpers, facts, receipts, prior assertions and restored
state. Within Access's local-bootstrap scenario, bootstrap establishes the
controller, Admin and `admin.userId` before session checks; authorization actors
are created after fresh-install assertions. Admin forms reuse authorization's
`authz_state()` protected snapshot for cancel checks and require untouched design
identity `u3` and restored installed retry/banner configuration. These fragments
share one controller/database and Python namespace; they are not independent gates.

Update a catalog row when its requirement, ownership, or scenario reference changes. A copy or spacing change usually needs only an update to the existing scenario and the MR impact record.

### Shared screens and shared components

Ownership follows the behavior. A dashboard's scan count belongs to Assurance; its build count belongs to Delivery; its navigation mechanics belong to Access. Opening the dashboard in one check does not prove every domain value on it.

Test shared mechanics once, then prove each distinct caller's binding. For example, an evidence drawer needs one comprehensive test of shared focus and close behavior. System, Compliance, and POA&M callers still need assertions for their different target identities, permissions, and return destinations. Those assertions belong in the existing workflows that reach the drawer. Reuse the shared-mechanics proof only where callers use equivalent containment and configuration. Add a focused interaction case when a caller changes dialog nesting, modality, focus ownership, or responsive layout.

A domain check can configure a resource as setup without repeating Access's complete form-maintenance assertions. Current-version server, builder, and agent restart recovery belongs to Delivery; Upgrade owns recovery specific to the supported version transition.

A connected cross-domain journey has one primary owner. An evaluation-to-finding round trip can remain in Delivery because the originating evaluation workflow owns the navigation. Assurance separately owns how finding evidence is produced and interpreted. Repeat assertions in both only when they protect distinct boundaries.

## 4. Required change procedure

### Step 1: Identify the change and its approval

Read the task, accepted domain contract, and applicable design. Classify each changed requirement as presentation, interaction, domain behavior, compatibility, or an implementation defect under an unchanged requirement.

For an updated design example, inspect added, changed, and removed sections, controls, fields, states, and interactions. Include shared components and styles. An unchanged entry-point file does not establish that its imported design is unchanged.

An agent MUST NOT treat every source-code edit as a product change or every design edit as a backend change.

### Step 2: Make the change-impact map

Before editing acceptance expectations, map each affected contract to its owner and existing scenario. Group related changes in one row; do not create a row for every DOM element.

For a shared component or token, inspect consumers and list the distinct layouts, data bindings, and navigation contracts affected. A small source diff can affect many screens.

Every changed accepted behavior must have a disposition: **retain**, **update**, **extend**, **add**, or **remove**. If existing coverage is sufficient, cite the scenario and explain which assertion proves it. No test change is a valid outcome when it is justified.

### Step 3: Update the smallest owning scenario

Retain assertions for unchanged requirements. Replace assertions when an approved requirement changes. Remove obsolete assertions or duplicate scenarios when an approved change makes them unnecessary. Record the requirement that authorizes each removal.

Before adding a scenario, answer these questions in the impact record:

1. Which observable behavior or failure condition is currently unproved?
2. Why can a small extension of the owning scenario not prove it clearly?
3. Which existing topology, fixture, or helper will the new scenario reuse?

A new scenario is justified by a distinct contract or state, not by the existence of a new task. An agent MUST NOT create overlapping mini-suites inside the four checks to evade the fixed inventory.

| Change | Minimum test maintenance |
| --- | --- |
| Layout, spacing, color, icon, or static-copy change | Update the affected design expectations and visual targets. Retain unchanged behavior and data assertions. |
| New control, navigation, selection, dialog, or return behavior | Extend the owning browser journey and assert its actual destination, state transition, or persistent effect. Add the relevant failure or unavailable state when it introduces distinct behavior. |
| New domain behavior, permission, evidence rule, or transport | Update the owner of that boundary and the affected UI binding. Use the real component for the integration result claimed. |
| Changed persisted schema or supported protocol compatibility | Update Upgrade when old supported data or clients are affected, plus the owner of the current-version behavior. |
| Refactor under an unchanged contract | Preserve expected behavior. Run the affected existing coverage; update internal test plumbing only when required. |
| Removed supported behavior | Remove or replace its obsolete assertions and catalog entries with the approval reference. Preserve historical/upgrade coverage if the compatibility contract still requires it. |

Internal algorithm branches and pure state transformations belong in Rust unit tests unless they create a distinct integration obligation.

### Step 4: Prepare deterministic, discriminating data

Reuse established fixtures when they can expose the changed requirement. Add only the variation needed to distinguish correct behavior from a plausible wrong implementation.

For example, exact-target navigation needs at least two distinguishable targets or revisions. A fixture in which every system has the same name, commit, status, and result cannot prove selection correctness.

CF's design handoff already defines `crystal-forge.fixtures.json` and its JavaScript wrapper. Reuse that fixture source for comparable presentation states. If the source fixture changes, keep its JSON and wrapper consistent. Key mappings by stable IDs rather than array position.

A small fixture subset or adapter is acceptable if it preserves identity and meaning. Keep one source for its underlying fixture facts. Do not maintain an unrelated copy of all mock registries in the tests.

### Step 5: Implement and verify the changed result

Use the normal production UI, APIs, and services for the claimed workflow. Maintain the task's live preview according to the existing repository instructions. Use focused local scenarios during implementation, then obtain a successful result for each affected owning check at the source revision submitted for review. Report missing obligations explicitly. The authorized TASK-490.24 source retirement does not establish replacement coverage.

Local results or CI can supply that final evidence. A focused run does not establish that the full owner passed. If a later edit changes a check's relevant inputs, earlier execution is insufficient. A valid Nix result for identical declared inputs can be reused; do not rebuild unchanged inputs for ceremony.

Run other owners when their contracts, shared application code, fixture semantics, or harness inputs are affected. A global stylesheet can affect several owners. A shared NixOS module, protocol, or migration can affect all four. A local visual change does not automatically require every integration lifecycle to run.

Report **passed**, **failed**, **blocked**, or **unverified**, with actual evidence. Do not report intended checks as executed checks.

## 5. What a scenario must prove

Each scenario needs a clear actor/scope, initial state, action or event, required outcome, and relevant failure conditions. Include the design reference for browser-visible assertions. These can be expressed in the test's name, short comments, and the linked requirement; a second complete prose script is unnecessary.

### Functional proof

For a changed mutation, prove the browser action, the authoritative effect, and the subsequent read. Examples include saved configuration surviving reload, a requested job actually executing, activation changing the target's running state, and an export download containing the intended records.

Observe only the layers needed for the contract. An unchanged backend does not require a new backend test for a spacing edit. A visible success message or HTTP 200 alone cannot prove a new mutation's completion.

For denied or failed actions, assert the prohibited side effect does not occur. For a successful write followed by a failed refresh, distinguish a committed operation from a rejected write so that retry behavior is correct.

### UI state obligations

Consider success, loading, empty, filtered-no-results, read error/retry, write error with draft preservation, denied/unavailable action, stale response, and return navigation. Exercise a state when the change affects its distinct behavior or when no existing scenario proves it.

Do not multiply every state by every role, theme, viewport, cache, and deployment mode. Use a representative shared-mechanic case and the smallest additional cases for genuinely different bindings or decisions.

Prefer accessible roles, labels, and explicit stable identifiers. Avoid assertions coupled to incidental CSS classes or DOM nesting. Use bounded waits for observable conditions. Do not add arbitrary sleeps, broad retries, or larger timeouts to conceal an unexplained failure.

An enabled action must complete its specified workflow. A feature that is intentionally unavailable must show the approved unavailable state. A dead button or fixture-only success must not be accepted as completed integration behavior.

### Independent expected results

Expected results must come from the accepted contract and deliberately authored input facts. Do not derive the expected count, classification, scope, or lifecycle state from the same CF response or function being tested.

Runtime-generated IDs and Nix store paths may be captured when necessary. Assert their relationships and provenance against independent input facts or the actual external result. Capturing a generated ID is different from accepting whatever semantic result the application returns.

Seed prerequisite or historical data when the scenario needs it. Do not seed the successful result of the action the scenario claims to test. For example, inserting a successful deployment record cannot prove agent activation.

Use real CF services, PostgreSQL, Nix, agents, scanners, and supported cache implementations for their claimed integration boundaries. Controlled external fixtures and fault injection may supply deterministic upstream data, delays, disconnects, and errors. They must not replace a successful CF workflow with a fabricated response. Name the injected condition in the scenario.

For CF assurance changes, preserve distinctions such as Current versus built/historical evidence, failed versus not checked, technical result versus enforcement, waiver versus remediation, and requirement coverage versus executable control count. Presentation changes do not authorize merging those concepts.

## 6. Visual baseline maintenance

A screenshot baseline records an accepted appearance. It does not establish why that appearance is correct. Every baseline change needs an identified approved reference or an approved change to the rendering environment.

Follow this procedure:

1. Select the affected approved reference component/state and the corresponding deterministic application state.
2. Use the shared pinned render environment: NixOS/browser version, fonts, viewport, device scale, locale, timezone, theme, and motion setting. Define a canonical desktop profile and a narrow profile once. Add another profile only for a distinct supported layout or behavior.
3. Compare the reference and implementation for required and unexpected sections, section order, merged/separate concepts, metadata, interactions, and visual hierarchy.
4. Update semantic assertions for changed content, order, relationships, and interactions. Retain unchanged identity, authorization, and persistence assertions.
5. Generate only the affected candidate baselines. Preserve reference, actual, and difference images as review evidence. Keep the reference revision/path, fixture revision, and render profile with the baseline or its existing test metadata.
6. Accept a candidate only after the comparison shows that it expresses the approved design. Include the comparison in the normal MR review so that unexplained deviations remain visible to the reviewer. An already approved design change authorizes the corresponding baseline update; it does not require another owner decision for each image.
7. Run normal verification with baseline updating disabled. A missing baseline, unapproved difference, failed semantic assertion, or missing mandatory scenario must not produce a passing check.

If the design and production renderer can consume equivalent fixture data, render the approved reference independently and compare it with production. If implementation screenshots are retained as regression baselines, establish their initial design conformance through that reference-to-implementation comparison. Do not generate expected and actual images from the same unreviewed application output.

Normalize only irrelevant nondeterminism. Prefer controlled data and clocks. Do not mask target identity, status, counts, permissions, evidence, or the changed region. Do not broaden pixel tolerances, remove regions, or disable snapshots merely to make a failure disappear.

A browser or font update can legitimately change many images. Treat that as a rendering-environment change, retain the reference comparison, and explain the affected scope. Do not combine an unrelated broad baseline refresh with a local feature change.

For a regression fix or newly changed behavior, demonstrate that the assertion distinguishes the previous wrong result from the required result when practical. Use the previous implementation or one small controlled wrong-state probe. Do not add a separate mutation-testing system or require replay of every historical release.

## 7. Evidence and completion

The MR must make the requirement-to-result chain reviewable: approved change, affected contract, owning scenario, assertion disposition, and execution evidence.

Include:

- The tested source revision and exact owning check commands.
- The executed scenario names and actual outcomes.
- For UI changes, the approved reference and relevant actual/difference images; a browser trace when needed to review an interaction or failure.
- The required server, database, VM, target, or external-service observation for the changed boundary.
- Every required case that is failed, skipped, blocked, or unverified.

The check must fail when any mandatory scenario fails. Do not catch failures and continue to an overall success. Do not commit focused-only execution, silently skip a required case, convert it to an expected failure, or narrow the fixture until the defect disappears.

The fixed count is not a claim that all software defects can be excluded. “Complete” means that every accepted in-scope workflow and distinct integration obligation has an executable owner and the required evidence. An unsupported feature or proposed future design is not silently added to that scope.

## 8. Keep the suite small and enforceable

Reuse the established VM topology. The proposed maximum is five VMs; ordinary UI changes must not add VMs or duplicate full deployment/scan setup. Smaller checks use only the nodes they need. Changes that require a new real boundary must explain the resulting topology and cost.

Test each distinct supported transport/authentication implementation, then reuse the common workflow proof. Do not form an exhaustive provider × builder strategy × deployment mode × user role × visual-state product.

When a new scenario replaces an older one, remove the obsolete duplicate in the same change. Reuse helpers for repeated mechanics without hiding the assertion or failure identity. Split long files when it improves diagnosis; keep the same check owner.

Once measured, retain the normal runtime and peak-resource figures for each check. Explain a material increase using comparable runs. Do not invent runtime estimates or enforce an arbitrary scenario-count ceiling.

CI must run every implemented integration entry point and, at final cutover, the four declared entry points, and retain their ordinary test reports. Review changes to check registration, selection, skipped cases, fixtures, and baseline tolerances. A small allowlist comparison can run at the start of the existing integration CI entry point to detect changes to the four-name inventory; it does not require a fifth flake check or a new validation service. Changes to that allowlist still need the approval described in Section 9.

Automated inventory enforcement cannot determine whether a design or product decision was approved. The short impact record and reference comparison are the review mechanism for that decision. Do not create a generated contract database or a separate coverage-preservation framework.

## 9. Exception: changing the four-check inventory

An additional top-level check requires both a new supported contract and a concrete need for an independent execution environment or lifecycle that the existing owners cannot cover reliably.

The proposal must identify:

- The newly accepted behavior and its approval reference.
- Why no existing owner can contain the proof as a scenario or variant.
- Required components, topology, isolation, runtime, and maintenance cost.
- Any coverage moved from an existing check, so that duplication is removed.

Examples might include a future supported multi-controller failover contract or a separately accepted long-running capacity requirement. These are illustrations, not additions to the present CF scope.

The owner must approve the inventory change explicitly. If the active task already grants that approval, cite it and proceed. A feature request or a design update alone does not authorize additional check entry points.

## 10. MR impact template

Use this short record in the existing task/MR workflow. Do not create a permanent report file for every change.

```markdown
## Integration contract impact

Approved requirement/design: <decision or exact reference revision + path/state>
Change: <old required result -> new required result, or unchanged requirement + defect>

| Contract / affected consumers | Owner and scenario | Disposition and assertions | Evidence |
| --- | --- | --- | --- |
| <stable ID; relevant callers/states> | <existing check; file/name> | <retain/update/extend/add/remove; what and why> | <tested revision; result; artifact> |

Fixture/baseline changes: <affected data/state/profile, or none>
New scenario justification: <unproved behavior; why existing scenario cannot cover it; reused setup, or none>
Removed/weakened assertions: <approved replacement/removal reason, or none>
Other owners: <affected boundary and verification, or why existing coverage is sufficient>
Verification: <exact commands and actual results>
Remaining gaps: <failed/blocked/unverified requirements, or none>
Top-level inventory: <unchanged, or explicit approved inventory decision>
```

For a small presentation change, one table row and a few short entries are sufficient. The record must explain the actual change; boilerplate claims that everything is covered are insufficient.

## 11. Worked example: an evaluation drawer design update

This is a hypothetical next design change. It is not a claim that the reference currently lacks these capabilities or that the production implementation has been tested.

Suppose the owner approves a new `EvalDrawer.jsx` revision that changes the policy-matrix header and control placement, and changes the finding round trip so that a selected bundle is restored on return.

The reference already includes bundle/control modes, filtering, finding identity, and restoration state. The change therefore extends the existing `delivery.evaluation.matrix` scenario under `integration-delivery`.

| Changed requirement | Update to the existing scenario |
| --- | --- |
| New matrix header and control placement | Assert required header content and section order; replace only the affected matrix visual targets after reference comparison. |
| Bundle selection survives a finding visit | Select a non-default bundle, set the approved column-filter state, expand a host/control, open the finding, and return. Assert the required selections and expansion state. Additional filter restoration requires an explicit design requirement. |
| Exact finding remains required | Assert system, evaluation revision, policy/control, and bundle/version identities against deliberately distinct fixture records. Do not relax these assertions because the layout changed. |
| Historical selection remains exact | Use another evaluation or move the current head so a latest/current fallback would return a distinguishably wrong record. Assert the selected historical identity remains intact. Displaying exact historical evidence does not grant current remediation or deployment authority. |
| Unchanged tabs and backend semantics | Retain their existing expectations. Do not regenerate their baselines or rerun a complete remediation lifecycle merely because the drawer changed. |

Use the smallest fixture extension that makes a wrong result visible, such as a second distinguishable bundle or evaluation. The approved domain contract determines counts and evidence meaning; the mock design does not invent those rules.

If only presentation and return navigation change, Delivery owns the changed scenario. If the same task changes evidence authorization or remediation meaning, update the corresponding Assurance scenario. If stored formats change, assess Upgrade. The check inventory remains four.

## 12. AGENTS.md rule

The repository's operational agent guide contains the following section. Keep the full contract in the knowledge base.

```markdown
## Integration test maintenance

Read `docs/knowledge/testing/integration-test-contract.md` before changing
an integration contract, browser-visible behavior, integration tests,
fixtures, or visual baselines.

The integration owners are `integration-access`, `integration-delivery`,
`integration-assurance`, and `integration-upgrade`. Extend the smallest
existing owning scenario first. Add a scenario only for a distinct
unproved behavior or state. A new feature, page, component, or regression
does not authorize a new top-level flake check.

Before editing acceptance expectations, record the approved requirement
or design revision, affected consumers, owning scenario, and assertion
disposition in the task/MR impact record. Use the approved design as the
UI reference. Keep data meaning, identity, permissions, persistence, and
evidence rules aligned with the accepted domain contract.

Update implementation, affected contract links, fixtures, assertions,
and approved visual targets together. Retain valid assertions; replace
or remove obsolete assertions only with an approved requirement reason.
Do not use current application output to approve its own expected result.

Every user-visible change must be exercised by its owning integration
check and have the required design comparison and MR images. Use focused
local checks during implementation, then obtain results for affected
owners at the review revision. Report failed, blocked, skipped,
and unverified cases explicitly. A preview is not proof that a check passed.

Changing the four-check inventory requires an explicit owner-approved
decision under the integration contract. Cite an approval already present
in the active task; do not ask for the same approval again.
```

TASK-490.1 added this section to `AGENTS.md`, replaced the instruction that every UI change must run the single `web-ui` check, and updated `docs/agents/verification.md` and the testing index. The live-preview, task isolation, screenshot, and honest-reporting requirements are unchanged.

**Transition rule.** Meaningful WIP owner checks may be discoverable before their full obligation set is implemented. Run the affected registered scenarios and owner check, and report missing obligations.

Implemented integration owners run automatically in GitLab. Under the explicit
TASK-490.24 decision, the nine legacy suite sources and their manual jobs are
retired now, before all four owners pass. This supersedes prior source-retention
conditions; it does not remove accepted requirements. Obsolete mocked or seeded
implementations do not become mandatory replacement requirements. The coverage
map distinguishes executed coverage, missing accepted obligations, obsolete
implementations, and lower-level assertions preserved outside `checks`.

**Final minimization rule.** The final flake check inventory contains only the
four product integration owners. Every independent verification package or CI
test job must have a distinct necessary purpose. Preserve useful assertions,
assign primary execution ownership, and consolidate or justify duplicate
execution. Developer tools remain package or app outputs. Final completion
requires complete accepted coverage, no task-specific VM proliferation hidden
beneath four aggregate names, and measured execution and resource costs.
The existing state-machine workflow owns consolidated server/database selections.
Six repeated ignored selections, 94 normal-library selections and two CVE-job
`composite_policy` executions are removed. This source audit does not establish
a full current-head run. The [component catalog](flake-checks.md#current-inventory-and-commands)
records exact homes and measured derivation counts; task notes retain execution
status. No replacement verification registry or aggregation framework is added.
The [retirement map](integration-coverage-map.md#legacy-retirement-map) records
source dispositions separately from coverage and CI selection.

The automatic owner job builds the normal owner aggregate, because a `.run` output can build successfully while its result records a failed scenario. It exports evidence from the scenarios that the owner registers and keeps the original build exit status when evidence collection follows a failed build.

## Source references

These sources establish repository context and tooling behavior. The four-check inventory and maintenance rules above are recommendations, not claims that these sources already prescribe that inventory.

- [Crystal Forge AGENTS.md](https://gitlab.com/crystal-forge/crystal-forge/-/blob/33de73b013b735304dfac9c69745a074a3e76ebd/AGENTS.md): task authority, design requirements, knowledge-base location, UI verification, and evidence reporting.
- [Systems design authority and status](https://gitlab.com/crystal-forge/crystal-forge/-/blob/33de73b013b735304dfac9c69745a074a3e76ebd/docs/design/CrystalForge/docs/crystal-forge-systems-design/systems-view-design-v0.1.md): approved UI authority, domain/presentation distinction, and treatment of design gaps.
- [Design fixture README](https://gitlab.com/crystal-forge/crystal-forge/-/blob/33de73b013b735304dfac9c69745a074a3e76ebd/docs/design/CrystalForge/fixtures/README.md): deterministic shared presentation data, stable IDs, and JSON/wrapper consistency. Mock records are not backend acceptance authority.
- [Design golden fixture knowledge guide](https://gitlab.com/crystal-forge/crystal-forge/-/blob/33de73b013b735304dfac9c69745a074a3e76ebd/docs/knowledge/testing/design-golden-fixtures.md): handoff location and fixture usage context; its stated status is not new execution evidence for this proposal.
- [EvalDrawer reference](https://gitlab.com/crystal-forge/crystal-forge/-/blob/33de73b013b735304dfac9c69745a074a3e76ebd/docs/design/CrystalForge/components/EvalDrawer.jsx): matrix modes, finding navigation, and restoration-state inputs used to ground the worked example.
- [Playwright visual comparisons](https://playwright.dev/docs/test-snapshots): snapshot generation, update behavior, rendering-environment sensitivity, and review of snapshot changes.
- [Playwright best practices](https://playwright.dev/docs/best-practices): user-visible assertions, isolation, stable locators, and bounded condition-based assertions.
