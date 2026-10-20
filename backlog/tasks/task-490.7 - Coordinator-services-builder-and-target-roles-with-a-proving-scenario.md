---
id: TASK-490.7
title: 'Coordinator: services, builder, and target roles with a proving scenario'
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-17 16:36'
updated_date: '2026-10-20 05:24'
labels:
  - testing
  - integration
  - coordinator
dependencies:
  - TASK-490.1
references:
  - packages/default/crates/cf-server/src/handlers/api/builders.rs
  - packages/default/crates/cf-builder/src/builder/api_client.rs
  - modules/nixos/crystal-forge/default.nix
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335'
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2964502717'
modified_files:
  - lib/integration-test/roles.nix
  - checks/integration-delivery/default.nix
  - checks/integration-delivery/README.md
  - checks/integration-delivery/fixtures/configuration.nix
  - checks/integration-delivery/scenarios/delivery-lifecycle.nix
  - checks/integration-delivery/scenarios/delivery-lifecycle.py
  - checks/integration-delivery/scenarios/fragments/source-build.py
  - docs/knowledge/testing/integration-coverage-map.md
  - .gitlab-ci.yml
parent_task_id: TASK-490
priority: high
type: task
ordinal: 519000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implements concrete shared services, separate API-only builders and a real NixOS target in the existing integration harness. The native Delivery owner explicitly registers one behavior-oriented delivery-lifecycle; no packet-specific VM, smoke gate, discovery framework or placeholder owner is permitted.

Current interfaces in roles.nix: services {} provides local smart-HTTP Git plus an authenticated SSH Nix store; builder { name, strategy ? "server_derivation" } provides the production API-only builder; target {} provides the real agent with runtime key provisioning. Controller/services/builders share immutable baseline preparation, not runtime outputs. OIDC, email, other cache fixtures and verified-source mode are not claimed implemented.

The source-build.py module proves an authored nonce-bearing Git commit, exact CF ingestion/native evaluation/attempt/derivation, authorized builder claim and actual offline Nix realization, cold-before/valid-after output, signed local publication and exact successful job/session/output identities. A real target registers and periodically reports its running generation. BuilderB establishes an independent real session after BuilderA completes, so this is not a competing-claim proof. Private credentials stay inside VMs; API-only builders have no DB section or remote controller DB connection.

Eight mandatory cases passed locally with the normal owner aggregate in the working tree based on fc0c5836. This is initial Delivery coverage, not completion of .8-.15. Later compatible ingestion, scheduling, publication, activation and recovery obligations extend this lifecycle through receipts. Independent scenarios never share live state.

The owner separately authorized TASK-495 after offline=true exposed an unsupported nix-store flag. Its independent product commit is applied without changing offline=true or weakening assertions. TASK-496 remains an observed heartbeat-classification defect; real periodic reporting does not prove the lightweight storage optimization. No other production repair, schema/design change, merge or deployment is authorized.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Documented services, builderA, builderB and targetA interfaces are implemented through lib.crystal-forge.makeIntegrationTestHarness { inherit pkgs inputs; }, including real fixtures, separate credentials, key provisioning and offline inputs
- [x] #2 The coordinator composes the disjoint behavior-named source-build.py module as a real commit→evaluation→build case in delivery-lifecycle; builderA output is independently absent before build
- [x] #3 Native Delivery owner explicitly registers delivery-lifecycle and its pkgs.linkFarm aggregate includes every registered scenario; no separate smoke gate, placeholder owner or custom discovery is required
- [x] #4 Later same-scenario cases consume earlier real outputs only through receipts; independent derivations never reuse live VM/database/output/receipt state
- [x] #5 Actual interface signatures and tested dependency/source revision, focused command, result and evidence are recorded after the real Delivery VM run
- [x] #6 A cold input-addressed output is rejected specifically at an enforced signature boundary when unsigned or signed by an untrusted key; it remains absent/invalid, then the correctly signed equivalent succeeds with exact contents and signature evidence. Transport, access and dependencies are independently proven; no trusted-user/import/content-addressed bypass.
- [x] #7 Controlled test-owned IPv4/IPv6 endpoints prove actual outside-boundary connection rejection with route/interface and boundary-hit evidence, while required local Git/API/SSH-store/agent traffic works. No deployed cache probe or isolation removal is permitted.
- [x] #8 The real agent establishes exact registered state, unchanged heartbeats correlate without duplicate full states or comparison errors, a genuine observed change creates new full-state evidence, and later heartbeats correlate to that new state after the authorized TASK-496 repair.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Resume in the existing coordinator worktree /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation, branch of the same name, base dev. User explicitly selected continuation in draft MR !335. The prerequisite corrective pipeline 2961382083 passed all seven required hosted jobs at c2b77aad. Coordinator lock is recorded for this worktree.
1. Implement concrete services (local Git and required real publication destination), API-only builder {name,strategy}, and target constructors in existing roles.nix. Instantiate controller/services/builderA/builderB/targetA only once in delivery-lifecycle. No production changes or speculative future fixture interfaces.
2. Author an offline locked source fixture and a runtime nonce-bearing Git commit. Preload only immutable inputs and baseline build dependencies, never the nonce-specific result. Require actual source ingestion/native evaluation and independently verify the authorized drv/output and exact attempt identity before allowing builderA to poll.
3. Use separately generated builder/target keys and real authenticated protocols. Require builderA actual Nix realization with absent-before/present-after nonce output, successful job/identity correlation, builderB distinct session, and target registration plus real heartbeat. Native builder requires publication after build: configure a real test-owned destination as a prerequisite, without claiming complete publication coverage.
4. Explicitly register the meaningful Delivery lifecycle in a native owner aggregate. Compose semantic source-build.py sections using existing receipt/driver helpers; no browser fragment unless necessary. Later compatible delivery cases extend this lifecycle.
5. Evaluate and build the normal owner, inspect all receipts/logs and correct only test plumbing. Record truthful failures without production fixes, then run hosted owner and update existing coverage map with exact evidence, topology and remaining obligations. Keep MR draft; no merge/deploy/Done.

Owner-selected continuation at refreshed clean f6439851. User explicitly corrects this task's lifecycle: its initial role/build proof and review findings may enter Review independently of future dependent task completion. The old criterion requiring .8-.15 contributions is removed from this foundation task only; those accepted obligations remain on the children/parent gate. Coordinator owns shared roles, composition, registration and integration. First batch closes enforced signatures (inspect pinned Nix trust semantics; use untrusted receiver path in an existing VM), controlled outside-VLAN IPv4/IPv6 connectivity with positive controls and actual boundary hits, and exact heartbeat/state-lineage assertions after separate TASK-496 integration. Preserve existing cases and all offline/cold-store constraints. Then execute selected .14/.15 activation/rollback and independent Assurance work; no further product fix without authority.

Resume network finding in the existing coordinator worktree at d11de059. Add a second controlled VLAN to the same five VMs, a dual-stack HTTP probe on services, and a narrowly scoped control UID permitted only to that probe. Assert real IPv4/IPv6 route selection, same-route positive control, product-actor rejection and exact firewall counter increments. Preserve NAT isolation and verify local Git/API/SSH traffic. Compose the behavior-named fragment after the existing source/build lifecycle; run the normal Delivery aggregate and inspect its raw result. No production/UI change or signature/activation completion claim in this bounded batch.

Signature finding: after normal publication, use the exact delivery.outputPath in a separate rooted Nix store and Unix-socket daemon inside the existing services VM. Run all destination copies as cache, explicitly untrusted by that daemon, and assert trusted=false. Generate receiver-only runtime signing keys; keep receiver policy fixed. Sign and copy dependencies only, prove target physically absent/invalid, reject target specifically for lack of a trusted signature, then sign the same target and repeat the same untrusted copy successfully. Compare NAR identity/references and nonce; require input-addressed output. This independently proves signature enforcement, not that the existing trusted SSH publisher enforces signatures. Strengthen initial agent checks to require repeated heartbeat rows linked to the exact registered state with no duplicate unchanged full states or comparison errors; genuine changed-state coverage remains for actual activation. Verify normal Delivery aggregate and document exact evidence.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Verified initial Delivery milestone

- Verified code: `bea0c29936c2220a8c1ba8dbfeb5750dacc18cde`.
- Hosted pipeline 2964502717 succeeded. All eight jobs passed: Access, Delivery, component, state-machine, CVE, dashboard, complexity and coverage.
- The hosted Delivery artifact records 8/8 mandatory cases passed, exact source/attempt/drv/job/builder-session/target identities, `outputAbsentBeforeBuild=true`, `outputRealized=true`, `buildCompleted=true` and `publicEgressDenied=true`.
- Hosted evidence: job17294404507 artifacts under `integration-evidence/integration-delivery/delivery-lifecycle/`. Target periodic reporting used full-state persistence; TASK-496 classification remains unresolved.
- Both normal aggregates passed locally at the same clean source: Access23/23 and Delivery8/8, exit0. Latest Delivery evidence: `/nix/store/61x9c3ii0zw0b6c4pzln0af1vcm8zhfd-vm-test-run-cf-it-delivery-delivery-lifecycle-run`; driver79.32s. Configured memory10GiB across five VMs; peak unmeasured.

## Actual interfaces and lifecycle

`services {}` supplies smart-HTTP Git and a signed/authenticated SSH Nix store. `builder {name,strategy?"server_derivation"}` supplies separate production API-only builders; `target {}` supplies the real client. Runtime keys remain in VMs; all later generated identities pass through receipts. BuilderB starts after BuilderA completes, so no competing-claim or fencing coverage is claimed. One controller/database and the same five roles serve the entire lifecycle. Independent scenarios never share live state.

## Cold preparation and history

The source generator's direct dependency fan-out measured2525->5. This is not a whole-owner recipe reduction. An opaque raw recipe closure worked warm but failed cold export on a bootstrap object before VM startup. The native baseline now prepares its complete build closure once for compatible roles. Cold preparation remains a minimization constraint, not an excuse to seed the nonce output or acquire dependencies during offline execution. Early Git, lock, signing and capacity setup failures remain historical evidence; no assertion was weakened.

TASK-495 was separately authorized, developed in its own worktree as6844d185 and applied asfc0c5836. offline=true remained unchanged. TASK-496 remains Backlog; P3/TASK-492 is also separate.

## Remaining task scope

Criteria1,2,3,5,6 are verified for the initial role/build milestone. Criterion4 concerns later functional contributions; those obligations remain unimplemented and are not marked complete. Keep this task In Progress and MR!335 draft. Further source/evaluation, scheduling, cache, activation and recovery behavior must extend appropriate semantic modules in this lifecycle, never a task-specific VM or file boundary. No merge/deploy/Done.

Preflight: existing coordinator branch/worktree is clean at f6439851, synced to origin; target origin/dev remains5ae7feab. Existing openai coordinator locks refer to this worktree. Dev has no tracked modifications; unrelated locks/session file remain untouched. The owner authorizes dependency-ready child selection for this assignment and TASK-496 only as the new product repair. No architecture reset or inventory repeat.

Network continuation verified locally on 2026-10-19 in coordinator worktree based on d11de05967b2ade5c7e1f833317039de134c307a (dirty source, not a clean-head or hosted claim). Added fixtures/network-isolation.nix, fixtures/network-probe.py and fragments/network-boundary.py; composed the new case after the existing eight cases in delivery-lifecycle.nix. Same five VMs, second VLAN only. Each controller/builder/target IPv4 and IPv6 probe proves eth2 routing, live same-route control UID response, failed product-actor connection and increment of the exact rejection-rule counter. Local Git/API/authenticated SSH still works; NAT isolation retained; no deployed cache probe. Criterion 7 is locally proven.

Exact gate command: nix build .#checks.x86_64-linux.integration-delivery --no-link -L --builders '' --max-jobs 1 --cores 2. Final exit 0, 9/9 cases, driver 77.08s. Evidence /nix/store/lra2iw4nmplfr1lycslrc2dh4nfgg0l9-vm-test-run-cf-it-delivery-delivery-lifecycle-run/result.json records all eight network receipts and localRoleTrafficPreserved=true. First executed attempt failed IPv6 positive control because the fixture blocked neighbor-discovery replies to global VLAN addresses; corrected only neighbor-discovery permissions and narrowed control exception to exact probe addresses/port. Failed run preserved at /nix/store/pmwnqnhijsa3yn0c1zi5p0camxv45jpy-vm-test-run-cf-it-delivery-delivery-lifecycle-run. Earlier build attempts hit 120s and 1200s tool timeouts before VM execution; neither is a pass.

Python AST parsing and Nix owner derivation evaluation passed. git diff --check and git diff --cached --check passed. nix build .#okf-knowledge --no-link -L --builders '' --max-jobs 1 --cores 2 exited 0: 58 tests, 215 Mermaid blocks parsed with 0 errors, 192 concepts validated. README and coverage map document the tested boundary, failure, evidence and remaining limits. Full flake check and new hosted pipeline were not run in this bounded batch. No new commit/push/MR change. Existing database-runner selections remain untouched. TASK-496 repair is applied; final run observed heartbeat representation, but permissive fallback assertion remains and criterion 8 is not closed. Signature rejection and exact real-agent state transition are next; activation/rollback/Assurance/Upgrade remain incomplete. Keep In Progress and existing lock.

Signature and unchanged-heartbeat continuation verified in the existing coordinator worktree based on d11de059 (dirty source). Added fixture signature-receiver.nix and fragment publication.py; no new VM or production edit. The exact CF output is copied into a separate rooted store inside services using a Unix-daemon client with trusted=false. All 727 dependencies were signed/copied/validated first. The input-addressed target failed specifically because it lacked a signature by a trusted key and remained physically absent and invalid. Only source signature metadata changed; the same copy then succeeded with identical NAR hash, size, references and nonce. Receiver policy stayed fixed. This proves AC6 independently of the existing trusted SSH publication account, not signature enforcement by that account.

Replaced periodic-report fallback assertion with two real agent heartbeat rows referencing exact initial state S1 and registered UUID, exactly one unchanged full state, and no database-comparison-error journal entry. Genuine S2 transition and later heartbeat remain unimplemented; AC8 stays unchecked. Normal Delivery aggregate command nix build .#checks.x86_64-linux.integration-delivery --no-link -L --builders '' --max-jobs 1 --cores 2 exited 0: 10/10, driver178.07s. Evidence /nix/store/i6h35avw0z6kkyci7zxv5f2rlz9i8c2d-vm-test-run-cf-it-delivery-delivery-lifecycle-run. Signature case89.1s; exact rejection diagnostic and accepted signatures retained in receipts.

Full nix flake check --keep-going --no-update-lock-file -L --builders '' --max-jobs 1 --cores 2 exited 0 on x86_64-linux. Both registered owners executed successfully: Access23/23 driver50.90s evidence /nix/store/q9qhgv1qh35lj3s71wcg8s84q0985kwr-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run; Delivery10/10 driver160.52s evidence /nix/store/y88mk7bplyb6hjxyqacsnsx8zhyf2dyn-vm-test-run-cf-it-delivery-delivery-lifecycle-run. Raw log /home/mcamp/.local/share/opencode/tool-output/tool_157024128001M1lC0NX34v0jkT. Incompatible aarch64-darwin/aarch64-linux/x86_64-darwin omitted; no all-systems claim. Passing registered subset does not establish absent Assurance/Upgrade owners or full accepted coverage.

Prepared launcher with nix build .#devScripts.state-machine-test --no-link --builders '' --option substituters https://cache.nixos.org --option extra-substituters '' --option accept-flake-config false --max-jobs 1 --cores 2 and fetched locked Cargo sources through nix develop .#sqlx with the same preparation settings. Then CARGO_NET_OFFLINE=true SQLX_OFFLINE=true nix run --offline --option substituters '' --option substitute false --option builders '' --option fallback false --option accept-flake-config false --max-jobs 0 --no-update-lock-file .#devScripts.state-machine-test -- -t=false up exited0. Actual 229 Rust summaries total517passed/0failed/0ignored; both TASK496 regressions passed and populated migration cases retained. Launcher used its fresh temporary cluster, not shared DB. Raw log /home/mcamp/.local/share/opencode/tool-output/tool_156e2d3f4001UOmCl7yY7BUZpg. Offline settings are not a host firewall-isolation claim.

Updated README, coverage map and current verification/selection counts (286 ignored-library plus231integration) in docs. Final nix build .#okf-knowledge --no-link -L --builders '' --max-jobs 1 --cores 2 exited0:58tests,215Mermaid blocks/0errors,192concepts. Python syntax checks, staged/unstaged whitespace checks passed. No commit/push/new hosted run/merge/deploy. Keep In Progress and lock: AC8 and broader accepted lifecycle work remain. User retracted the unrelated glyph/Mermaid request; no implementation was performed for it.

Original verified local increment committed/pushed as3db47c8953e3cf1f29473be7595a660d69e6a988, with productionfix separately attributable tod11de059 (original5c8f5ee8). Exact clean-commit hostedpipeline2965867360 passed all8jobs; downloadedAccess23/Delivery10artifacts matchsourceRevision and database517/0/0across229summaries. These hostedresults are notlateractivationverification.

AC8 now has actual deployment change evidence in connected local3db47c89-dirty run: originalstate1/H1,2 -> CF-managedgeneration1/state2/H6,7 -> distinctCF-managedgeneration2/state3/H9,10. No duplicatefullstates/comparisonerrors; exactregisteredUUIDf1addf9f-3f84-4b8b-9598-c244d29ae307, running/profile/marker/request/historyandbrowserreloadverified. Latestownerexit1 overall13passed1failed solelymandatoryrollbackeligibility TASK499; no rollbackcompletion. Artifact /nix/store/mrs8rm4yvf5lk9vklqs01qvmzp1zl9n7-vm-test-run-cf-it-delivery-delivery-lifecycle-run. Alltenpreviouscasesinclsignaturenegative/positiveandcontrolleddualstackegresspassedagain. Broaderparentnotcomplete; keepInProgresswhilecurrentrequiredgatefailed.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-10-19 02:12
---
Backlog maintenance only: source-build.py is the proposed stable behavior-oriented source path, not proof of an implemented module or passing Delivery case. AC #2 retains the real commit→evaluation→build and independently absent-before-build obligations. No .cjs fragment is required for this non-browser proof. Coordinator retains the broader services/builder/target implementation context above for later work. No status change or startDelivery is authorized by this maintenance update.
---
<!-- COMMENTS:END -->
