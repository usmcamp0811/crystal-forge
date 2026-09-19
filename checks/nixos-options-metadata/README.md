# NixOS Options Metadata Check

This is a pure Nix evaluation check (no build, no VM, no network) over the
`nixos-options-metadata` package, which extracts declared-type metadata for
NixOS options that the Config explorer needs to render values safely.

## What it verifies

- The extracted metadata list is sorted by option path (`lib.sort
  builtins.lessThan`), which downstream binary-search/lookup code depends
  on.
- Known representative options resolve to the expected declared
  `value_type`: `networking.firewall.enable` is `boolean`,
  `networking.networkmanager.dns` is a non-empty `enum`,
  `boot.consoleLogLevel` is `integer`, `networking.hostName` is `string`, and
  `networking.extraHosts` is `lines`.
- The packaged `share/crystal-forge/nixos-options.json` file exists and is
  non-empty.

## Why it is a separate check

The metadata this package produces is a build-time extraction from the
NixOS module system, not from any Crystal Forge runtime code path. A pure
Nix `assert`-based check is the cheapest and most direct way to catch a
regression in the extractor or an unexpected upstream NixOS option type
change, without booting a VM or exercising the server.

## Run it

```sh
nix build .#checks.x86_64-linux.nixos-options-metadata --print-build-logs
```

This evaluates near-instantly beyond the cost of building the
`nixos-options-metadata` package itself; there is no test script, VM, or
network access during the check phase.

## Out of scope

- This does not verify how the server or web UI consume this metadata
  (declared type rendering, redaction, search) — only that the extracted
  data itself has the expected shape and known-good values.

## CI

Not present in the `.gitlab-ci.yml` `flake-check` matrix by name; it runs as
part of a broader `nix flake check` where applicable.
