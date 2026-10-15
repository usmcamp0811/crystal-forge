# Builder and server runtime packaging

```sh
nix eval --raw .#checks.x86_64-linux.builder-evaluator-packaging.drvPath
nix build --no-link -L .#checks.x86_64-linux.builder-evaluator-packaging
```

Use `path:.#` while required new source files remain untracked in a shared task
worktree. This check needs the builder and core server packages; it does not
build the browser UI or start host services.

The check asserts that `pkgs.nix-eval-jobs.nix` is first in both service PATH
lists and that the evaluator-bound Niks3 package is present. It checks standalone
component wrappers and the public builder wrapper. Runtime probes require
`command -v nix` to resolve to the exact evaluator Nix and require
`command -v niks3` and `niks3 --help` to succeed. The Niks3 wrapper must bind the
same evaluator Nix. Finally, the packaged Nix version must match the version
reported by `nix-eval-jobs` itself.

## Bulk evaluator resource contract

The pinned package is exactly `nix-eval-jobs` **2.34.3**, linked against
Nix **2.34.8**. In upstream v2.34.3 `src/worker.cc`, `shouldRestart` compares
each worker's own peak RSS after the job response, then requests a worker
restart. This package does not implement aggregate-worker killing or
retry-alone semantics. A more recent deployed binary may have different
semantics; verify its version and source before attributing that behavior to
this package. The Nix library version is not the `nix-eval-jobs` version.

See [Bulk evaluator memory planning and timeouts](../../docs/knowledge/evaluation/bulk-evaluator-resource-planning.md)
for the adaptive resource contract. The
aggregate target derives a per-worker threshold; it is not hard containment.
The service cgroup remains the hard memory boundary. Automatic sizing, worker
resolution, and outer deadlines do not change evaluator Nix selection, the
root flake pin, or the guarded netrc patch below.

The working boundary now includes independently detected finite visible-ancestor
`memory.high` and `memory.max` alongside physical memory. `memory.high` is a
reclaim/throttling boundary, not an OOM-kill limit. Normal explicit overrides
remain exact; the CF isolated-recovery cap is derived from the original plan.
See [Adaptive recovery](../../docs/knowledge/evaluation/bulk-evaluator-adaptive-recovery.md)
for pressure evidence, shared deadlines, cleanup quarantine, and partial
resource failure. CF recovery is separate from upstream v2.35.4's aggregate
scheduler and 200-millisecond RSS sampling; the package remains v2.34.3.

The module proof defines seven valid evaluator cases: `defaults`,
`explicitNull`, `explicit12288`, `custom`, `workersZero`, `lowerBounds`, and
`upperPercent`. It parses generated TOML to assert null omission, automatic
reserve/percentage and timeout defaults, explicit integer preservation, custom
values, and valid boundaries. It also defines 25 invalid option type/range
cases across the six evaluator options. These cases extend the existing proxy
and config-path checks. This description records assertions in the check
source, not a passing result for the current worktree. A packaging pass alone
does not prove runtime memory detection, automatic worker resolution, timeout
cleanup, adaptive pressure recovery, partial-result retention, or newer upstream
memory semantics. Review those runtime gates separately for the exact source.

## Guarded Basic read runtime

The CF overlay applies `packages/default/patches/nix-cf-netrc-authority.patch`
to the evaluator's modular Nix component scope. Both `nix-eval-jobs` and its
native CLI link the same patched libraries. The upstream Nix version remains
`2.34.8`; the root flake pin and unrelated `pkgs.nix` remain unchanged.
`pkgs.crystal-forge.default.evaluatorNix` exposes that exact CLI. Server and
builder wrappers, service PATH lists, and the Niks3 wrapper use the same CLI.

The packaging check requires the registered `cf-netrc-authority` setting and
its empty default. Basic-read owners must detect the registered setting on the
actual executable before advertising support or preparing credentials:

```sh
nix --extra-experimental-features nix-command config show cf-netrc-authority
```

An unknown-option warning followed by success is not feature evidence. The
JSON settings listing must contain `cf-netrc-authority`. Unpatched runtimes
must not advertise Basic-read capability or run a Basic read.

For one Basic read child, pass `--option netrc-file /protected/absolute/path`
and `--option cf-netrc-authority https://read.example:443`. The second value is
a nonsecret origin, not a credential URL. The setting rejects userinfo,
non-HTTPS origins, queries, fragments, and paths other than `/`. Curl's URL
parser normalizes host case, IDNA, IPv6, and effective ports. Explicit port 443
and the omitted HTTPS port are equivalent. IPv6 zones remain part of identity.
Every transfer must match that origin before network access or netrc selection,
including absolute NAR URLs. Guarded transfers permit only GET/HEAD reads and
reject uploads and S3 URLs. All redirects are disabled and HTTP 3xx responses,
including 304, fail with static errors. The owner must use fresh metadata when
conditional-cache reuse would otherwise produce 304.

An empty guard preserves upstream redirects, system netrc, mTLS store
parameters, and ordinary transfers. Set the guard only in a Basic-read child;
never in global Nix configuration or a write/S3 subprocess. The guard does not
change signature verification, DNS policy, trust roots, or credential lifetime.

Pinned Niks3 `v1.6.0` (commit `c29f3641de064545d75f00318cd45bea2b4ea1d0`)
selects `ReadProxyHandler` in `server/server.go` with `--enable-read-proxy`.
`server/proxy.go` streams cache objects from S3 without redirecting clients;
only the root landing-page path delegates to a redirect handler. The
production-shaped fixture enables this streaming mode. Object reads are
source-compatible with the guard; redirect-based cache topologies are not.
This source inspection is not a passing native Basic transport test.

The Basic fixture owner must extend the existing `niks3-cache` VM check with
same-origin reads, same-origin redirect refusal, foreign-host/port and downgrade
refusal, absolute NAR URLs, and absence of credential forwarding. Packaging
feature checks do not establish those behavioral results. Changing the native
Nix runtime invalidates earlier six-variant evidence for this source; rerun the
authoritative gates after the sibling Basic helper/API/UI work is ready.

The Niks3 override changes only its Nix dependency. The pinned upstream Niks3
version remains 1.6.0.

## Forwarded HTTPS module validation

Run the focused proof independently of the Rust packaging build:

```sh
nix eval --json .#checks.x86_64-linux.builder-evaluator-packaging.moduleValidation.evaluationResults
nix build --no-link -L .#checks.x86_64-linux.builder-evaluator-packaging.moduleValidation
```

The full packaging check depends on this proof. Both checks are sandbox
derivations, not VM tests. The focused proof evaluates the real exported NixOS
module and forces the host's `system.build.toplevel.drvPath` assertion consumer:

| Global enable | Server enable | Trust flag | Proxy CIDRs | Result |
| --- | --- | --- | --- | --- |
| true | true | false | empty | valid |
| true | true | true | empty | fails with exact option names and narrow direct-peer guidance |
| true | true | true | loopback `/32` and `/128` | valid |
| false | true | true | empty | valid; module inactive |
| true | false | true | empty | valid; server inactive |

The assertion requires explicit configuration. It does not infer a CIDR or
relax runtime peer/header verification. For Traefik on the same host **connecting
over loopback**, the immediate configuration is:

```nix
services.crystal-forge.server = {
  trust_forwarded_builder_https = true;
  trustedProxyCidrs = [ "127.0.0.1/32" "::1/128" ];
};
```

A remote Traefik instance requires its actual backend socket-peer IP as `/32`
or `/128`, not the builder or client IP. The proxy must strip incoming
`X-Forwarded-Proto` and set exactly one `https` value. `Forwarded` alone and
protocol lists do not authorize builder secrets or private agent cache reads.

### Generated configuration and startup path

The proof executes each evaluated server pre-start config generator with only
`mkdir`, `cp`, and `chmod` intercepted to redirect writes into the build sandbox.
It parses the resulting TOML with `toml2json` and compares typed values with
`jq`; it does not infer configuration correctness from source-text matches.
It proves these mappings for false/empty and true/explicit-CIDRs:

| Nix option under `services.crystal-forge.server` | Generated TOML under `[server]` |
| --- | --- |
| `trustedProxyCidrs` | `trusted_proxy_cidrs` array |
| `trust_forwarded_builder_https` | `trust_forwarded_builder_https` boolean |

`crystal-forge-server.service` runs the evaluated `ExecStart` shell wrapper.
That wrapper exports `CRYSTAL_FORGE_CONFIG=/var/lib/crystal-forge/config.toml`
before it executes the configured package's `bin/server`. The pre-start generator
copies the module-generated store TOML to that same path. `configPath` is a
read-only reporting option; this module has no `configFile` override option.
Setting `CRYSTAL_FORGE_CONFIG` in the unit environment or its optional
`EnvironmentFile` does not change the file selection because the wrapper
overwrites that variable. Replacing `ExecStart` bypasses this module contract.

The proof runs the unchanged wrapper with a recording `bin/server` package. It
checks the path received by the binary, including an inherited alternative path,
and checks that nested environment overrides reach the binary unchanged. It
does not start the production server or test HTTP authorization.

Read-only loader audit: `cf-config/src/config/mod.rs::CrystalForgeConfig::load`
adds the TOML source first, then `Environment::with_prefix("CRYSTAL_FORGE")`
with `separator("__")`. With locked `config` 0.15.19, the prefix separator also
defaults to `__`. Thus `CRYSTAL_FORGE__SERVER__TRUST_FORWARDED_BUILDER_HTTPS`
can override the TOML flag; `CRYSTAL_FORGE_SERVER__...` is not a nested override.
The loader does not enable environment list parsing; configure the CIDR array
in TOML rather than assuming a comma-separated environment value works.
The Nix assertion validates Nix option values, not subsequent runtime
environment changes. Without the module wrapper, the Rust loader honors
`CRYSTAL_FORGE_CONFIG`; its fallback path is
`/var/lib/crystal_forge/config.toml` (underscore).

The proof output retains `assertions.json`, generated `.toml` and parsed `.json`
files, destination paths, and recording-binary runtime/override results for
inspection. Existing evaluator-Nix selection and wrapper probes remain required
by the full packaging check.

## CI

The `.gitlab-ci.yml` `flake-check` matrix includes
`CHECK_NAME: builder-evaluator-packaging`. The generated job
`flake-check: [builder-evaluator-packaging]` is blocking on merge requests and
`main`, uses the existing `nix` runner tag, and builds
`.#checks.x86_64-linux.builder-evaluator-packaging`. Matrix membership does not
establish a pass; review the job result for the exact commit under review.
