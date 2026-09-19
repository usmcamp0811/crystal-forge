# STIG Module Unit Tests

This check is a pure Nix evaluation — no VM, no build, no network — of
`lib.crystal-forge.mkStigModule`, the helper that lets a STIG control set
NixOS option values at a priority that wins over ordinary module
definitions.

## What it verifies

Nine assertion-based tests, each calling the production
`lib.crystal-forge.mkStigModule` implementation directly (not a copy),
covering:

- A plain STIG value (`mkOverride 1`) beats an ordinary conflicting
  definition.
- STIG's `mkForce` beats a user's `mkForce` (STIG's priority 1 beats
  `mkForce`'s priority 50).
- A bare `mkBefore` in `stigConfig` is wrapped whole at priority 1 and its
  ordering semantics (appearing before a same-priority definition) survive.
- A nested `mkDefault (mkBefore ...)` wrapper — the pwquality pattern — is
  unwrapped and rewrapped correctly.
- Override wrappers whose content is an attrset (the AIDE
  `mkDefault { text; mode; }` pattern) preserve every key, both with and
  without the `mkDefault` wrapper.
- A full `evalModules` run reproduces and closes the original TASK-398
  crash: `mkForce true` inside `stigConfig` must not produce a rejected
  nested override wrapper.
- `mkIf` wrappers preserve their condition and are not incorrectly applied
  when the condition is false.
- `mkMerge` wrappers, both at the leaf level and at the whole-`stigConfig`
  level, merge every element and still beat a conflicting definition.

## Why it is a separate check

`mkStigModule`'s priority/override/order/conditional/merge handling is
subtle NixOS module-system logic that a full NixOS VM build would exercise
only indirectly and slowly. A pure `evalModules` unit test against a minimal
scaffold module isolates this logic and fails fast and specifically when the
override-unwrapping logic regresses.

## Run it

```sh
nix build .#checks.x86_64-linux.stig --print-build-logs
```

Pure Nix evaluation; this is one of the fastest checks in the repository.

## Out of scope

- This does not exercise any real STIG control module (for example, the
  AIDE STIG control itself) — only the shared `mkStigModule` mechanism
  against a minimal scaffold module built for these tests.

## CI

Not present in the `.gitlab-ci.yml` `flake-check` matrix by name; it runs as
part of a broader `nix flake check` where applicable.
