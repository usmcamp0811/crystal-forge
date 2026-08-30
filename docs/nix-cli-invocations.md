# Nix CLI Invocations by Service

Every `nix`, `nix-store`, `nix-env`, and `nix-eval-jobs` call made by each
Crystal Forge binary, grouped by service. Calls listed in execution order.

---

## Server (`crystal-forge server`)

### `handlers/api/builders.rs`

| # | Purpose | Command | Source |
|---|---------|---------|--------|
| 1 | Build authorization manifest for a job's `.drv` closure | `nix-store --query --requisites <drv>` | line 189 |
| 2 | Sign closure paths before cache publish | `nix store sign --recursive --key-file <path> <paths...>` | line 818 |
| 3 | Push closure to non-Attic cache | `nix copy --to <url> [--refresh] [--compression <algo>] <paths...>` | line 892 |
| 4 | Verify `.drv` is valid in local store | `nix-store --check-validity <drv>` | line 2346 |
| 5 | Enumerate closure paths for archive export | `nix-store --query --requisites <drv>` | line 2362 |
| 6 | Export derivation closure as binary archive (streamed per chunk) | `nix-store --export <paths...>` (spawned, stdout piped) | line 2481 |
| 7 | Verify `.drv` is valid before cache publish | `nix-store --check-validity <drv>` | line 2925 |
| 8 | Enumerate closure paths for cache publish | `nix-store --query --requisites <drv>` | line 2941 |
| 9 | Verify builder pushed path to cache | `nix path-info --store <url> <store-path>` (+ `NIX_CONFIG`) | line 3104 |

### `flake/eval.rs`

| # | Purpose | Command | Source |
|---|---------|---------|--------|
| 10 | Discover `nixosConfigurations` in a flake | `nix flake show --json <uri>` | line 69 |
| 11 | Force-refresh flake cache | `nix flake metadata --refresh --json <uri>` | line 170 |

### `flake/commits.rs`

| # | Purpose | Command | Source |
|---|---------|---------|--------|
| 12 | Discover all `nixosConfigurations` attr names for a commit | `nix eval --json --apply builtins.attrNames <target>` | line 1000 |

### `models/evaluate_with_policies.rs`

| # | Purpose | Command | Source |
|---|---------|---------|--------|
| 13 | Resolve `.drv` → output store path | `nix-store --query --outputs <drv>` | line 87 |
| 14 | Evaluate systems with deployment policies | `nix-eval-jobs --expr <nix-expr> --impure --meta --workers <n> --max-memory-size <n> [--check-cache-status]` | line 291 |

### `derivations/eval.rs`

| # | Purpose | Command | Source |
|---|---------|---------|--------|
| 15 | Resolve `.drv` → output store path | `nix-store --query --outputs <drv>` | line 55 |
| 16 | Resolve `.drv` → output store path (static) | `nix-store --query --outputs <drv>` | line 85 |
| 17 | Check if agent is enabled for a NixOS config | `nix eval --json --impure --expr <expr>` (uses `builtins.getFlake`) | line 242 |

### `derivations/build.rs` (shared with builder)

| # | Purpose | Command | Source |
|---|---------|---------|--------|
| 18 | Build derivation (systemd-scoped) | `nix-store --realise --log-format internal-json --add-root <path> --indirect <drv>` | line 206 |
| 19 | Verify store path signatures | `nix store info <path> --json` | line 348 |
| 20 | Build derivation (direct fallback) | `nix-store --realise --log-format internal-json <drv>` | line 595 |
| 21 | Resolve built `.drv` → output path | `nix-store --query --outputs <drv>` | line 606 |

### `cf-server/derivations/utils.rs`

| # | Purpose | Command | Source |
|---|---------|---------|--------|
| 24 | Resolve `.drv` → output path | `nix-store --query --outputs <drv>` | `get_store_path_from_drv` |
| 28 | Count dependency derivations (filter `.drv` requisites and exclude the top-level system derivation) | `nix-store --query --requisites <drv>` | `calculate_dependency_build_plan` |
| 29 | Calculate dependency build work with the effective substitute and offline configuration | `nix-store --realise --dry-run <drv> <build-options...>` | `calculate_dependency_build_plan` |

The dependency build-plan command counts only derivations in Nix's build
section. It does not count fetched paths. A successful plan with no output is
zero work. Any unrecognized nonempty output fails closed. Command failure,
timeout, malformed output, and unavailable legacy data remain distinct from a
completed zero-build plan. The server persists a generation-bound terminal plan
before queue activation. Recovery replaces only expired calculations. This is a
server-side estimate from the server store at evaluation time. Remote builders
can have different store contents, substituters, Nix settings, or architecture,
so the estimate does not claim to equal one remote build attempt's exact work.

### `builder/worker.rs`

| # | Purpose | Command | Source |
|---|---------|---------|--------|
| 31 | Verify `.drv` valid before creating GC root | `nix-store --check-validity <drv>` | line 394 |

---

## Builder (`crystal-forge builder --api`)

### `src/bin/builder.rs`

| # | Purpose | Command | Source |
|---|---------|---------|--------|
| 32 | Evaluate verified source flake attribute → drvPath | `nix eval --raw --no-write-lock-file --option allow-import-from-derivation false <attr>` | line 1132 |
| 33 | List all store paths in a `.drv`'s recursive closure | `nix-store --query --requisites <drv>` | line 1748 |
| 34 | Check which store paths are invalid in local store | `nix-store --check-validity --print-invalid <paths...>` (batched, 1024/chunk) | line 1793 |

### `builder/api_client.rs`

| # | Purpose | Command | Source |
|---|---------|---------|--------|
| 35 | Import full derivation archive from server | `nix-store --import` (stdin piped from HTTP response) | line 707 |
| 36 | Import delta derivation archive from server | `nix-store --import` (stdin piped from HTTP response) | line 879 |

---

## Agent (`crystal-forge agent`)

### `src/bin/agent.rs`

| # | Purpose | Command | Source |
|---|---------|---------|--------|
| 37 | Resolve store path → `.drv` (fast path) | `nix-store --query --deriver <path>` | line 90 |
| 38 | Resolve store path → `.drv` (fallback) | `nix path-info --json <path>` (parses `"deriver"` field) | line 104 |

### `deployment/agent.rs`

| # | Purpose | Command | Source |
|---|---------|---------|--------|
| 39 | Pull system closure from binary cache | `nix copy --from <url> [--refresh] [<options...>] <store-path>` | line 462 |
| 40 | Set new NixOS generation | `nix-env --profile /nix/var/nix/profiles/system --set <store-path>` | line 732 |

---

## Totals

| Service | `nix` | `nix-store` | `nix-env` | `nix-eval-jobs` | Total |
|---------|-------|-------------|-----------|-----------------|-------|
| Server  | 10    | 17          | —         | 1               | 28    |
| Builder | 1     | 4           | —         | —               | 5     |
| Agent   | 2     | 1           | 1         | —               | 4     |
| Shared  | —     | —           | —         | —               | (7 counted in both server+shared) |
| **All** | **13**| **22**     | **1**     | **1**           | **37** |
