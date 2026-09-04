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
| 14 | Evaluate systems with deployment policies | `nix-eval-jobs --expr <nix-expr> --impure --meta --apply 'derivation: derivation.meta.policies' --workers <n> --max-memory-size <n> [--check-cache-status]` | line 291 |

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

### `derivations/utils.rs` (shared with builder)

| # | Purpose | Command | Source |
|---|---------|---------|--------|
| 22 | Enumerate all drvs in a build closure | `nix path-info --derivation --recursive <drv>` | line 392 |
| 23 | Quick check if drv outputs exist | `nix path-info --json <drv>` (exit status) | line 423 |
| 24 | Resolve `.drv` → output path | `nix-store --query --outputs <drv>` | line 433 |
| 25 | Enumerate all drvs in closure (with cache status) | `nix path-info --derivation --recursive <drv>` | line 453 |
| 26 | Resolve `.drv` → output path | `nix-store --query --outputs <drv>` | line 495 |
| 27 | Check if output path is already built | `nix path-info <store-path>` (exit status) | line 507 |
| 28 | Count closure packages (step 1: enumerate drvs) | `nix-store --query --requisites <drv>` | line 533 |
| 29 | Count closure packages (step 2: drv → output) | `nix-store --query --outputs <paths...>` (batched) | line 575 |
| 30 | Count closure packages (step 3: check cache) | `nix path-info --json <paths...>` (batched) | line 619 |

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
