# Assurance CVE producer composition

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
| `servicesModule` | Function `{ sslCertificate, sslCertificateKey, listenAddress ? "0.0.0.0", port ? 443 }` returning a NixOS module for the existing services VM. TLS paths are strings to coordinator-provisioned files. |
| `feedCommands.normal` / `.fault` | Root shell commands for the services VM. Atomically select the normal/fault document root. No nginx restart is needed. |

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

# Build the vulnerable closure through CF, then record the six build receipts.
assurance_cve_completed("vulnerable", builderA)
# FIRST real CF scan + production UI evidence; no deployment/rollback dependency.
with case("cve-first-real-scan-production-ui"):
    run_browser(controller, "cve-scan")

# Activate vulnerable output and persist the actual deployment receipt.
assurance_cve_current("vulnerable")
with case("cve-first-current-finding-navigation"):
    run_browser(controller, "cve-current")
# Downstream triage/POA&M cases can now consume this exact finding baseline.

# Build corrected in this same scenario; do not activate it yet.
assurance_cve_completed("corrected", builderA)
assurance_cve_built_not_deployed()
with case("cve-current-versus-built-clean-production-ui"):
    run_browser(controller, "cve")

# Activate corrected, then persist its actual deployment receipt.
assurance_cve_current("corrected")
assurance_cve_historical("vulnerable")
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
deterministic injected execution failure. No such terminal fault producer is
provided by these NVD feeds. Keep that acceptance gap open. The helper checks
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
post-activation clean and historical UI coverage remain unexecuted obligations;
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
