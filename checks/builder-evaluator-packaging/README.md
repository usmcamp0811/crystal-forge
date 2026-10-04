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
