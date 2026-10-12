# Deterministic cold/warm configuration discovery

Snowfall automatically exports this directory as
`checks.<system>.nixos-discovery-reliability`. Run the focused check with:

```sh
bash packages/ci/public-cache-build.sh \
  'path:.#checks.x86_64-linux.nixos-discovery-reliability' --no-link -L
```

The `runCommand` sandbox owns the loopback HTTP server, generated credentials,
Nix stores, and fetch caches. No service runs on the host. The fixture does not
fetch nixpkgs, change global caches, or measure an external download.

The exact local root contains a complete lock for a private tarball input. The
test ingests the root with `nix store add-path --name source`, verifies the stored
NAR hash and lock bytes, removes the staging directory, and passes the ingested
root's NAR-qualified `path:` reference to pure discovery and the production
`primary_evaluation.nix` expression. Each separate store receives the same exact
source name and bytes. The input supplies configuration names, so discovery must
fetch it. Endpoint counters require zero unexpected routes; the endpoint serves
only the input archive and cannot fetch the root.
The endpoint requires HTTP Basic authentication through a mode-0600 netrc file.
Before credentials are configured, real Nix discovery in a fresh store/cache
must fail with HTTP 401 within 15 seconds and without starting a delayed
authenticated response. Credentials never enter command arguments or retained
logs.

Each authenticated archive response waits 65 seconds. Real discovery under the
historical 60-second deadline times out in a separate disposable store/cache.
The Python timeout path sends `killpg(SIGKILL)` and uses `communicate` with a
10-second bound to wait for the direct child and collect its pipes before the
next run. It does not assert that the entire process group is absent or exercise
the Rust cleanup acknowledgement and lock-retaining quarantine contract.
Discovery with
a 300-second outer deadline succeeds cold and exceeds 60 seconds. A warm retry
must finish within 10 seconds without another HTTP request. This generous warm
bound tolerates CI scheduling while remaining far below the controlled delay.

A mismatched root NAR hash must fail. Discovery and policy evaluation must leave
the lock bytes and root NAR hash unchanged. `nix-eval-jobs` evaluates both
discovered systems through the normal policy expression with pure evaluation and
import-from-derivation enabled. It must return derivation paths and policy
markers without refetching the input.

A separate ingested root throws from `nixosConfigurations`. Discovery must
report that genuine Nix error without timing out or fetching an input. A root
with a missing private-input lock entry must fail both pure discovery and normal
policy evaluation without fetching or changing its lock. The check accepts
either a process failure or a JSON evaluation error from `nix-eval-jobs`.

The output contains `report.json` with measured cold, warm, and policy times and
per-command stdout/stderr. Generated credentials are checked before diagnostics
are retained. This check proves the real Nix source/input/evaluator contract; it
does not invoke the Rust discovery coordinator or prove advisory-lock ordering,
cancellation quarantine, terminal database state, or configuration validation.
Those behaviors require the parent's Rust integration regressions.
