# Assurance CVE producer composition

## Executable composition

The native owner now registers `scenarios/evidence-remediation-lifecycle.nix`.
Its Python body creates a fresh Assurance source/database/VM lifecycle, using
only immutable setup code from Delivery. The local `9f53fade-dirty` owner run
executed both real builds, scanner leases, deployments, Current and remediation
continuity, terminal rejection, transient requeue/recovery, and production UI.
The gate remains failed on the approved historical `read-only` cell copy versus
production `Historical inventory`, and on a failed scan's false-clean zero
summary. Do not report a passing owner or full parity.

The actual fault composition uses normal replacement-build post-build intents,
not the manual-rescan examples below. Manual intents lack producing-session
affinity in this remote-only topology. The idle builder unit is frozen while
its invocation-owned upstream cache is removed and services switches feeds;
thaw preserves its actual producing session. The packaged server's optional
local scanner executable is masked only in that server unit's mount namespace.
Coordination/maintenance still run; the unchanged builder scanner executes.
The test CA includes strict key-usage constraints; TLS verification stays on.
The initial never-scanned configuration explicitly disables both CF and the
instrumentation-declared agent unit, so build admission and scan intent do not
occur. The following vulnerable and corrected revisions enable the real agent.

Current/result provenance and exact commands are in the
[coverage record](../../../../docs/knowledge/testing/integration-coverage-map.md).
The helper interfaces below describe available producer functions, not proof
that every example is used by the composed scenario.

This fixture belongs to `assurance.scanning.cve` in the fresh
`evidence-remediation-lifecycle` scenario. Sol owns roles, source ingestion,
builds, deployments, TLS/routing, composition and registration. Importing this
directory creates upstream files and package inputs, not CF scan results.
Do not import Delivery runtime IDs, databases, receipts or output state.

## Exports

Import `default.nix` with the scenario's `pkgs` and optional `years` (default
2021 through 2031, inclusive). The returned attributes are:

| Export | Contract |
| --- | --- |
| `facts` | Parsed, independently authored `facts.json`; install as `FACTS["cve"]` and browser `facts.cve`. |
| `packages.vulnerable` | Real `cf-assurance-demo-1.0` derivation. Include in the first system closure. |
| `packages.corrected` | Real `cf-assurance-demo-2.0` derivation. Replace 1.0 in the second system closure. |
| `feedRoot` | Gzip NVD 2.0 tree under `feeds/json/cve/2.0/`. Only 2026, modified and recent contain the advisory. Other supported years are valid empty feeds. |
| `faultFeedRoot` | Same filenames, valid gzip containing malformed JSON. No scanner executable is replaced. |
| `terminalFaultFeedRoot` | Same filenames, valid gzip/JSON with the matching demo advisory's CVSS score set to `11`. Real vulnix emits the score; the real builder rejects it deterministically. Use only with the vulnerable closure. |
| `servicesModule` | Function `{ sslCertificate, sslCertificateKey, listenAddress ? "0.0.0.0", port ? 443 }` returning a NixOS module for the existing services VM. TLS paths are strings to coordinator-provisioned files. |
| `feedCommands.normal` / `.fault` / `.terminalFault` | Root shell commands for the services VM. Atomically select the normal, transient-fault or terminal-fault document root. No nginx restart is needed. |

`facts.json` has exact values: `scannerVersion = "vulnix 1.12.4"`,
`package = "cf-assurance-demo"`, `vulnerableVersion = "1.0"`,
`correctedVersion = "2.0"`, `cveId = "CVE-2026-49001"`, `cvssV3 = 9.8`,
and the authored advisory `description`. Expected vulnerable findings are
exactly this one CVE/package/version with critical severity; corrected findings
are exactly empty. Neither expectation is calculated from CF responses.

## Services and builder setup

Example composition inside Sol's existing scenario (paths relative to its
`scenarios/` directory):

```nix
let
  cveData = import ../fixtures/vuln-data { inherit pkgs; };
  cveSpec = ./fragments/cve.cjs;
in {
  # Merge into the existing nodes/facts/browserSpecs, not another scenario.
  nodes.services.imports = [ (cveData.servicesModule {
    sslCertificate = "/run/cf-it-tls/nvd.crt";
    sslCertificateKey = "/run/cf-it-tls/nvd.key";
  }) ];
  facts.cve = cveData.facts;
  browserSpecs = {
    cve = cveSpec;
    "cve-scan" = pkgs.writeText "cve-scan.cjs" ''
      exports.run = require("${cveSpec}").scan;
    '';
    "cve-current" = pkgs.writeText "cve-current.cjs" ''
      exports.run = require("${cveSpec}").current;
    '';
  };
}
```

The coordinator must provision the server certificate/key before nginx starts,
with a certificate SAN for `nvd.nist.gov`. Route that hostname to services on
the builder and trust the issuing test CA in Python requests. Set the builder
unit's `REQUESTS_CA_BUNDLE` to the trusted bundle if needed. Do not disable
certificate checks. Open only the selected TLS port. This module does not own
the CA, hostname mapping, other vhosts or CF services.

The packaged scanner's default URL is
`https://nvd.nist.gov/feeds/json/cve/2.0/`. It requests gzip JSON directly, not
`.meta` files or an API. Pin the VM date within 2026–2031. Fresh caches request
the six years `[current year - 5, current year]`; an update after two hours but
within seven days uses `modified`. A warm cache performs no request for two
hours. The vhost disables conditional responses and open-file caching so that
the next requested file follows the selected root.

Enable the real API builder's `services.crystal-forge.build.cve_scanning_enabled`
and the registered builder's server-side scanning capability through the real
registration/configuration workflow. Keep its actual scanner/version unchanged.
The first scan must be the exact producing job's `post_build` scan, executed by
that builder/session. Ensure another scanner or local fallback does not win the
claim. These are setup obligations, not permission to insert scan rows.
Include only the intended demo version in each runtime closure. Supply no
whitelist that hides the authored advisory.

## Same-lifecycle receipts and calls

Load `fragments/cve.py` once into the shared Python driver namespace after the
existing setup defines `literal(value)` (SQL string literal quoting). It uses
`controller`, `FACTS`, `case`, `expect`, `expect_equal`, `http`, `sql_json`,
`wait_for`, `receipt`, `receipts` and `shlex` from the shared driver/setup.
`session` defaults to the same-lifecycle `delivery-admin` authenticated jar;
pass a different same-scenario administrator jar explicitly when applicable.

For each `variant` (`vulnerable` or `corrected`), record these keys under
`assurance.<variant>.` from actual operations in this scenario:

| Input receipt | Type and origin |
| --- | --- |
| `systemId` | UUID string of the registered target system. Both variants use the same system. |
| `derivationId` | Integer CF NixOS derivation ID from its real evaluation. |
| `outputPath` | Absolute Nix output store path from the exact completed build. |
| `jobId` | UUID string of that producing build job. |
| `builderId` | UUID string of the real producing/scanning API builder. |
| `builderSessionId` | UUID string of its actual registered session for that job/scan. |
| `deployment` | Required only for Current: object `{systemId, derivationId, outputPath, generation, stateId}`. `generation` is a positive integer from the activated target; `stateId` is the string form of its actual persisted agent-report row ID. Other fields bind the exact same system/output/derivation. |

Call at real lifecycle boundaries:

```python
# After evaluation, before a build creates a post_build intent (not merely
# before releasing the scanner): record systemId and derivationId first.
assurance_cve_missing("vulnerable")
with case("cve-missing-production-ui"):
    run_browser(controller, "cve-missing")

# Build the vulnerable closure through CF, then record the six build receipts.
assurance_cve_completed("vulnerable", builderA)
# FIRST real CF scan + production UI evidence; no deployment/rollback dependency.
with case("cve-first-real-scan-production-ui"):
    run_browser(controller, "cve-scan")

# Activate vulnerable output and persist the actual deployment receipt.
assurance_cve_current("vulnerable")
with case("cve-first-current-finding-navigation"):
    run_browser(controller, "cve-current")
# These real same-scenario sessions need read scope for this system.
assurance_cve_permissions("assurance-viewer", "assurance-operator")
with case("cve-viewer-production-ui"):
    run_browser(controller, "cve-viewer")
# Downstream triage/POA&M cases can now consume this exact finding baseline.

# Build corrected in this same scenario; do not activate it yet.
assurance_cve_completed("corrected", builderA)
assurance_cve_built_not_deployed()
with case("cve-current-versus-built-clean-production-ui"):
    run_browser(controller, "cve")

# Activate corrected, then persist its actual deployment receipt.
assurance_cve_current("corrected")
assurance_cve_historical("vulnerable")
with case("cve-activated-clean-production-ui"):
    run_browser(controller, "cve-clean")
with case("cve-historical-production-ui"):
    run_browser(controller, "cve-historical")
with case("cve-read-error-and-retry-production-ui"):
    run_browser(controller, "cve-read-error")
```

`completed()` reads the actual builder closure and package deriver, then checks
the exact job's sole post-build scan. Required proof includes schema 1, real
vulnix identity, matching job/builder/session/target, live lease at completion,
execution UUID, server-sealed SHA-256 digest, explicit closure provenance,
authored counts and the complete immutable observation set. Remote
`unverified_remote` provenance remains distinct from `server_local_verified`.
The helper then checks the exact scan-detail API.

Output keys under each variant are `scanId` and `executionId` (UUID strings),
`closureProvenance` (enum string), `packageOutputPath` and
`packageDerivationPath` (store paths), `scanEvidence` (the SQL-selected sealed
scan/build/lease identity object in `cve.py`), and `observations` (the exact
ordered immutable observation array). Vulnerable observations contain
`canonical_cve_id`, `canonical_package_name`, `observed_package_name` =
`cf-assurance-demo-1.0`, `observed_package_version` = `1.0`, independently
resolved `observed_derivation_path`, `is_affected = true`,
`is_whitelisted = false`, `whitelist_reason = null`, and
`detection_method = "vulnix"`. Corrected observations are empty.

`current()` adds `currentScanId` only after checking both the receipt's report
and the latest persisted report, `view_current_cve_authority` and Current API
inventory. `built_not_deployed()` adds
`assurance.corrected.builtNotDeployedProven = true`. Explicit inventories use
`target=exact_derivation&target_id=<integer>` and remain `legacy`, read-only,
with `current_state = null`; this API term does not mean an undeployed build
has ever run. Current uses `exact` / `exact_current_scan`. Both select the
expected source UUID, schema-1 representation, authored versions and CVSS.

Browser `scan` emits `assurance.cve.browserExactScanId`. `current` additionally
emits `assurance.cve.browserCurrentScanId`. Full `run` additionally emits
`assurance.cve.browserBuiltNotDeployedScanId`; that corrected scan is not called
historical. Screenshots are `cve-exact-scan-details`,
`cve-current-vulnerable`, `cve-built-clean-read-only`, and
`cve-finding-navigation-current`, as applicable to the selected entry point.
The shared browser runner retains the screenshot/result and failure trace.

## Fault and recovery

Perform the fault only after normal UI/remediation baseline cases. Stop or
pause all competing scanners using coordinator-owned controls. On the isolated
scenario builder, stop `crystal-forge-builder.service`, confirm no scanner is
running, then remove only `/var/lib/crystal-forge/.cache/vulnix` (the default
under the unit's actual `HOME=/var/lib/crystal-forge`). If the unit uses another
HOME or an explicit `--cache-dir`, inspect it and clear that isolated cache
instead. `XDG_CACHE_HOME` alone does not change vulnix's default. Never clear
the host/development instance cache or any CF database.

On services, execute `cveData.feedCommands.fault`; restart the isolated builder
and call `assurance_cve_fault_rescan("vulnerable")`. This POSTs the real exact
rescan API with CSRF and checks a fenced `attempt_requeued` diagnostic caused
by `vulnix process failed`. It adds `faultRescanId` and `faultExecutionId`.
The scan remains pending/in progress, schema 0, with no sealed digest; earlier
same-target completed evidence remains the inventory source. Retain the fault
until this assertion returns, then restore `feedCommands.normal` in `finally`.
Do not restore the normal feed between the requeue event and the assertions.

Malformed upstream JSON makes pinned vulnix exit **1**, which the production
builder classifies as **transient**. It does not prove a terminal failed scan.
`assurance_cve_failed(variant, scan_id)` is a separate assertion for a genuinely
deterministic injected execution failure. The separate terminal feed below
provides that input. The helper checks
failed/schema-0 evidence, no digest/observations, and the fenced
`attempt_failed` event; it adds `failedScanId`.

Recovery can complete the requeued scan and make it the newest eligible source.
Receipts are write-once. Do not rerun `completed()` or claim the old `scanId`
remains newest after recovery. Record a separate recovery identity and assert
it in the coordinator's later cases.

## Verification boundaries and authority

`verify-offline.py` parses every authored gzip with the real pinned NVD parser,
loads an isolated temporary parser cache, and executes the packaged vulnix CLI
against both real fixture outputs. Its dead-loopback mirror fails if that
warm-cache execution attempts an update. This check starts no servers and
touches no CF database. It proves parser/matching/CLI identities, not cold HTTPS,
real CF leases, deployment or production rendering.

Acceptance authority is SC1 handoff revision 3 at `327d03b6`,
`docs/design/CrystalForge/docs/crystal-forge-systems-design/system-cves-chunk-1.md`,
as superseded for Current action by the normative
`docs/design/CrystalForge/cve-poam-evidence-continuity-design-spec.md`, especially
Sections 2.5 and 29. UI references at this worktree base `d11de059` are
`components/SystemDetail.jsx` (`CvesTab`, `CveScenarioScopeBar`),
`components/ScanningView.jsx` and `components/CvesView.jsx` under that design
tree. Existing assertions retain the accepted evidence meanings. Added early
entry points extend execution timing; they do not approve production deviations.
The first exact scan can be inspected independently of activation/rollback.
Current still requires actual activation and agent observation. Failed/missing,
post-activation clean and historical UI exports remain unexecuted obligations;
the existing full browser entry point proves vulnerable Current versus built
clean only when actually run. Screenshots alone do not prove design conformance.

Sol must compose and execute the focused gate and retain its exact revision,
result and browser artifacts:

```sh
nix build .#checks.x86_64-linux.integration-assurance.scenarios.evidence-remediation-lifecycle -L
```

No lifecycle run or browser success is claimed by the producer-only checks.

### Reproduce the cheap scanner check at this pin

Run from the task worktree. This includes the untracked fixture through an
explicit local import; Git-backed flake source filtering does not include
untracked files automatically.

```sh
nix build --impure --no-link --print-out-paths --expr '
  let f = builtins.getFlake (toString ./.);
      pkgs = f.inputs.nixpkgs.legacyPackages.x86_64-linux;
      v = import ./checks/integration-assurance/fixtures/vuln-data { inherit pkgs; };
  in [ v.feedRoot v.faultFeedRoot v.packages.vulnerable v.packages.corrected ]'

nix develop --command nix shell --impure --expr '
  let f = builtins.getFlake (toString ./.);
      p = f.inputs.nixpkgs.legacyPackages.x86_64-linux;
  in p.python3.withPackages (_: [ p.vulnix ])' --command python3 \
  checks/integration-assurance/fixtures/vuln-data/verify-offline.py \
  --scanner /nix/store/bvq5j3kxzjrmmw95yb2fi4xmxfwybxi6-vulnix-1.12.4/bin/vulnix \
  --feed-root /nix/store/8yk8qdny3y41iynzzrq7d1796d3wzaly-assurance-nvd-feeds \
  --fault-feed-root /nix/store/smz8g0milq2kjl9cvags932s4vdrkp4z-assurance-invalid-nvd-feeds \
  --vulnerable /nix/store/ngmjz6k50r5nx6ca4f18xyy94n72v307-cf-assurance-demo-1.0 \
  --corrected /nix/store/qcm1b1lcn5rqw1q99f7n7xnp00w5qj5l-cf-assurance-demo-2.0
```

Both commands exited 0 on 2026-10-20 at base `d11de059` with these producer
files. The scanner subprocess exits were 2 (one exact authored finding) and 0
(`[]`, corrected). Invalid gzip payloads were rejected by the real NVD parser;
this run did not execute the cold-download fault CLI path.

## Bounded extension interface (producer `b1c8485b`, integration `0f7f94dd`)

The existing `scan`, `current`, and full `run` exports retain their signatures
and timing. Register each additional browser phase using the same adapter as
`cve-scan`; for example:

```nix
"cve-missing" = pkgs.writeText "cve-missing.cjs" ''
  exports.run = require("${cveSpec}").missing;
'';
```

| browserSpecs key | `cve.cjs` export | Required lifecycle boundary and new receipt |
| --- | --- | --- |
| `cve-missing` | `missing` | After `missing("vulnerable")`, before build admission. Emits `assurance.cve.browserMissingDerivationId` (integer). |
| `cve-viewer` | `viewer` | Vulnerable Current proven. `facts.cveViewer = {username, password}` must identify a real Viewer with read scope for this system. Emits `browserViewerScanId`. |
| `cve-clean` | `clean` | Corrected activated and `current("corrected")` passed. Checks completed-empty System CVEs and exact `/scanning` execution. Emits `browserCleanScanId`. |
| `cve-historical` | `historical` | After `historical("vulnerable")`; requires corrected Current. Checks original source, authored CVE, read-only, no triage, and exact selection after reload. Emits `browserHistoricalScanId`. |
| `cve-read-error` | `readError` | Original vulnerable source still newest for its exact target. Aborts only its inventory GET transport, then removes interception before clicking Retry. Asserts actual retried API source/rows and no API writes. Emits `browserReadRetryScanId`. |
| `cve-failed` | `failed` | After `terminal_fault_rescan()`, before retry. Checks original source plus failed attempt, visible failure notice, exact `/scanning` diagnostic execution and retry control. Emits `browserFailedScanId`. |
| `cve-retry` | `retry` | After failed UI evidence and normal-feed restoration/cache reset. Clicks real Admin retry. Checks HTTP 202, exact target and fresh intent. Emits `browserRetryScanId`. |

All `browser*ScanId` receipts above use the prefix `assurance.cve.` and are
UUID strings. Each phase signs in using the real UI and records its screenshot
through the shared runner. No success response is mocked. `readError` injects
only a browser transport failure and never changes stored evidence.

New Python interfaces (default administrator jar remains `delivery-admin`):

- `assurance_cve_terminal_fault_rescan(session=...) -> scan_id`: enqueues a
  new vulnerable-target rescan and waits for the exact deterministic failure.
- `assurance_cve_failed(variant, scan_id, session=..., expected_error=None)`:
  existing assertion extended with optional exact error, real scan-detail API
  and execution diagnostic checks. Adds `failedScanId`, `failedExecutionId`
  (UUID strings) and `failedError` (string) under `assurance.<variant>.`.
- `assurance_cve_permissions(viewer_session, operator_session)`: both real
  authenticated jars must have scoped system read access. Asserts Current
  inventory matches the authored vulnerable source, diagnostics GET is 403,
  exact rescan POST with valid CSRF is 403, and denied requests create no scan.
  These are Admin-only scanner operations, not Operator triage operations.
- `assurance_cve_retry(session=...)`: after `cve-retry`, checks the emitted
  retry UUID exists as a new manual intent on the exact derivation. Checks the
  old failure's status/execution/class/error/digest remain unchanged. Admission
  is not completion; this helper does not claim a repaired scan is complete.
- `missing()` adds `assurance.<variant>.missingProven = true`;
  `historical()` adds `assurance.<variant>.historicalProven = true`.

### Terminal fault and recovery sequence

Keep this after downstream baseline consumers. Use the isolated scenario
builder only. Competing remote scanners and server-local fallback must not
claim the rescan. The coordinator owns those controls. Stop the builder unit,
confirm scanner exit, and clear its actual vulnix cache as described above.
Switch services to `feedCommands.terminalFault`, resume the real builder, then:

```python
try:
    scan_id = assurance_cve_terminal_fault_rescan()
    with case("cve-terminal-fault-production-ui"):
        run_browser(controller, "cve-failed")
finally:
    services.succeed(cve_terminal_restore_command)  # feedCommands.normal
    # Coordinator stops the isolated builder, confirms scanner exit, clears
    # only its upstream cache, then resumes it before the recovery retry.

with case("cve-real-admin-retry-production-ui"):
    run_browser(controller, "cve-retry")
assurance_cve_retry()
```

`cve_terminal_restore_command` is the coordinator's literal rendering of
`cveData.feedCommands.normal`, not a new fixture export. Restore the normal
feed even on failure. Reset the builder cache **again** before recovery: a
warm invalid-score archive otherwise persists for two hours. Do not issue a
second retry while another intent is active; this phase requires `enqueued=true`.
After repaired completion, the retry scan can become the newest eligible
vulnerable-target source. Original write-once `scanId` remains audit identity,
not a claim that the original source stays newest.

Mechanism and source rationale at producer base:

1. `default.nix:terminalAdvisory` changes only authored upstream
   `metrics.cvssMetricV31[0].cvssData.baseScore` to `11`. All year/update files
   carry that advisory so a cold or incremental request sees the fault.
2. Packaged vulnix 1.12.4 `vulnerability.py:Vulnerability.parse` converts the
   score with `float()` without a range check. The real CLI emits `11.0` for
   `cf-assurance-demo-1.0` and exits 2. It still emits `[]`/exit 0 for 2.0;
   therefore this fault requires the vulnerable closure, not the clean one.
3. `cf-builder/src/builder/cve_scanner.rs:parse_vulnix_output` accepts exit 2.
   `assemble_result` rejects a score outside `0.0..=10.0` with
   `Deterministic` / `vulnix returned an invalid CVSS score`. `execute_claim`
   calls the real failure API rather than completion.
4. `cf-server/src/queries/cve_scan_leases.rs:fail_remote_cve_scan` fences the
   real builder/session/execution/lease, persists `failed`, `execution_outcome
   = failed`, `failure_class = deterministic` and `attempt_failed`. It clears
   lease fields and never seals a successful digest/observation set.
5. Missing store outputs/materialization failures, ordinary scanner non-0/2
   exits and timeouts are transient. They are not substitutes for this case.

Only upstream fault data is authored. No executable, CF result, scan status,
finding, lease or database row is replaced. The real scanner **succeeds at
execution**; the real builder's evidence validator makes the scan terminal
failed. This is a controlled invalid scanner-input/result fault, not proof of
a vulnix crash. Cold HTTPS transport, CF persistence, and rendered-browser
results require Sol's lifecycle execution.

### Extension verification

Executed through the task's Nix environment; no servers or VMs were started:

```sh
nix develop --command nix build --impure --no-link --print-out-paths --expr '
  let f = builtins.getFlake (toString ./.);
      pkgs = f.inputs.nixpkgs.legacyPackages.x86_64-linux;
      v = import ./checks/integration-assurance/fixtures/vuln-data { inherit pkgs; };
  in [ v.feedRoot v.faultFeedRoot v.terminalFaultFeedRoot v.packages.vulnerable v.packages.corrected ]'

nix develop --command nix shell --impure --expr '
  let f = builtins.getFlake (toString ./.);
      p = f.inputs.nixpkgs.legacyPackages.x86_64-linux;
  in p.python3.withPackages (_: [ p.vulnix ])' --command python3 \
  checks/integration-assurance/fixtures/vuln-data/verify-offline.py \
  --scanner /nix/store/bvq5j3kxzjrmmw95yb2fi4xmxfwybxi6-vulnix-1.12.4/bin/vulnix \
  --feed-root /nix/store/8yk8qdny3y41iynzzrq7d1796d3wzaly-assurance-nvd-feeds \
  --fault-feed-root /nix/store/smz8g0milq2kjl9cvags932s4vdrkp4z-assurance-invalid-nvd-feeds \
  --terminal-fault-feed-root /nix/store/yq6793fml7y3p65vzccc70yb9pbsk7gx-assurance-invalid-cvss-nvd-feeds \
  --vulnerable /nix/store/ngmjz6k50r5nx6ca4f18xyy94n72v307-cf-assurance-demo-1.0 \
  --corrected /nix/store/qcm1b1lcn5rqw1q99f7n7xnp00w5qj5l-cf-assurance-demo-2.0

nix develop --command nix shell --impure --expr \
  '(builtins.getFlake (toString ./.)).inputs.nixpkgs.legacyPackages.x86_64-linux.nodejs' \
  --command node --check checks/integration-assurance/scenarios/fragments/cve.cjs
```

Build and real CLI checks exited 0. Both normal and terminal feeds produced
exit 2 for 1.0 and exit 0 for 2.0; exact CVE, package, deriver and scores were
asserted. Every terminal gzip was parsed by the real parser. No cold download
was exercised: this component check uses a temporary parsed upstream cache
and a dead-loopback mirror. It does not claim CF terminal failure persistence.
The authoritative lifecycle gate remains unexecuted by this worker. Preserve
correct assertions on any product conflict and report the exact defect to Sol.

Additional exact successful verifier commands (all exit 0):

```sh
nix develop --command nix shell --impure --expr '(builtins.getFlake (toString ./.)).inputs.nixpkgs.legacyPackages.x86_64-linux.nodejs' --command node -e 'const assert = require("node:assert/strict"); const m = require("./checks/integration-assurance/scenarios/fragments/cve.cjs"); assert.deepEqual(Object.keys(m).sort(), ["scan", "current", "run", "missing", "historical", "clean", "failed", "retry", "readError", "viewer"].sort()); for (const fn of Object.values(m)) assert.equal(typeof fn, "function"); console.log("PASS: CommonJS module and 10 phase exports");'

nix develop --command python3 -c 'import ast,json; from pathlib import Path; root=Path("checks/integration-assurance"); paths=[root/"scenarios/fragments/cve.py",root/"fixtures/vuln-data/verify-offline.py"]; [ast.parse(p.read_text(),filename=str(p)) for p in paths]; json.loads((root/"fixtures/vuln-data/facts.json").read_text()); paths += [root/"scenarios/fragments/cve.cjs",root/"fixtures/vuln-data/README.md",root/"fixtures/vuln-data/default.nix"]; assert all(line.rstrip()==line for p in paths for line in p.read_text().splitlines()); print("PASS: Python AST, facts JSON and changed-file whitespace")'

nix develop --command nix-instantiate --parse checks/integration-assurance/fixtures/vuln-data/default.nix
git diff --check
```

Source comparison `git diff b1c8485b 0f7f94dd --stat --` for the scanner,
lease-failure query, CVE/scanning handlers, CVE component and scanning/fleet
views returned no changes. Their inspected contracts apply to the stated
integration mapping. No product conflict was runtime-tested by this worker.
