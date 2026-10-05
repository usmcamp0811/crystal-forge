---
type: Testing Guide
title: Design golden fixtures contract
description: Pointer and status record for the design handoff's golden fixtures README, which defines the deterministic crystal-forge.fixtures.json snapshot that the design example, the fixture seeder, and the screenshot checks share.
tags:
  - crystal-forge
  - testing
  - fixtures
  - design-handoff
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:58:00-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/design/CrystalForge/fixtures/README.md at commit 3b23d36f"
    title: Crystal Forge — golden fixtures
---

# Design golden fixtures contract

This concept is a navigation and status record. The authoritative text is the
retained file
[docs/design/CrystalForge/fixtures/README.md](../../design/CrystalForge/fixtures/README.md).
That file stays at its path because the design handoff tree is read by path by
Nix packages, fixture seeding, and checks, and later design handoffs overwrite
the tree in place.

## What the README specifies

`crystal-forge.fixtures.json` is a canonical, deterministic snapshot of every
data registry that the design example renders from (its `data-*.js` mocks). It
is the shared contract between the HTML design example and the Dioxus port.
The README has these sections:

| README section | Content |
| --- | --- |
| Opening summary | The file is deterministic (seeded RNG, `_meta.rngSeed = 1337`, byte-identical regeneration) and complete (a stated count of systems, builds, evaluations, CVEs, flakes, policies, compliance bundles, and caches). |
| The design example reads this file too | `crystal-forge.html` loads the wrapper `crystal-forge.fixtures.js` (sets `window.__CF_FIXTURES`) before the `data-*.js` modules. Each registry prefers the fixture and falls back to its generator. |
| Suggested CI use | Two directions: deserialize the JSON into Rust structs and assert rendered output (recommended), or scrape rendered values and compare them with the fixture. Assertions key on stable ids (`sys-*`, `eval-*`, `CVE-*`, `fl-*`), never on array position. |
| Regenerating | The JSON and the `.js` wrapper are derived from the mock modules with `Math.random` seeded to 1337, and both must be regenerated together. |
| Top-level shape | The key list: `_meta`, `environments`, `flakes`, `systems`, `builds`, `evaluations`, `cves`, `policies`, `compliance`, `caches`, `scanning`, `admin`, `hardening`. |
| Entity fields | The asserted fields of System, Build, Eval, Cve, Flake, Policy, Bundle, Cache, ScanRow, and HardeningService. |
| Notes | Relative timestamps are frozen strings, and color fields are literal design tokens. |

Key decisions recorded in the README: determinism through a seeded RNG, one
byte-identical data source for the mock and the port, and assertions by stable
id.

## Implementation status and evidence

Status: implemented, with stale counts in the README text.

- The server fixture seeder reads the JSON.
  `packages/default/crates/cf-server/src/fixtures/seed.rs` and
  `packages/default/crates/cf-server/src/fixtures/mod.rs` consume it when
  `FIXTURE_JSON_PATH` is set. See the
  [fixture seeding developer guide](fixture-seeding.md).
- `checks/ui-screenshots/default.nix` and `checks/ui-screenshots/routes.js`
  read the JSON. See the [flake checks catalog](flake-checks.md).
- `checks/web-ui/design-parity/generate-design-targets.js`,
  `apps/generate-design-targets/default.nix`, `devenv.nix`, and
  `packages/devScripts/default.nix` also reference the file.

> **Status:** the README's stated counts differ from the JSON at the migration
> base commit. The README says 13 policies and 4 compliance bundles. The JSON
> at that commit has 848 entries in `policies` and 9 in `compliance`. The
> README counts for systems (35), active and historical builds (6 and 40),
> active and historical evaluations (4 and 50), CVEs (48), flakes (5), and
> caches (5) match. A later verification pass must decide whether to correct
> the retained README. The `_meta.rngSeed` value 1337 matches.

## Related concepts

- [Fixture seeding developer guide](fixture-seeding.md)
- [Flake checks catalog](flake-checks.md)
- [Web UI check runbook](web-ui-check.md)
