# Crystal Forge — golden fixtures

`crystal-forge.fixtures.json` is a **canonical, deterministic snapshot** of every data
registry the design example renders from (the `data-*.js` mocks). Use it as the shared
contract between the HTML design example and your Dioxus port: feed the same JSON into
both, render, and assert the outputs match.

- **Deterministic** — generated with a seeded RNG (`_meta.rngSeed = 1337`), so every
  regeneration produces byte-identical output. Safe to commit and diff in CI.
- **Complete** — 35 systems, 6 active + 40 historical builds, 4 active + 50 historical
  evaluations, 48 CVEs, 5 flakes (with per-flake commit lists), 13 policies, 4 compliance
  bundles, 5 caches, per-system scan history, and the admin/server registries.

## The design example reads this file too

`crystal-forge.html` now loads `crystal-forge.fixtures.js` (a classic-script wrapper that
sets `window.__CF_FIXTURES` to the exact bytes of the JSON) **before** its `data-*.js`
modules. Every registry declaration prefers the fixture and falls back to its original
generator only if the fixture is absent:

```js
const SYSTEMS = (typeof __fx === "function" && __fx("systems")) || HOSTS.map(buildSystem);
```

So the HTML mock and your Dioxus port read the **same bytes** — edit the JSON and the
design example reflects it on reload (regenerate the `.js` wrapper afterward; see below).
This also makes the example fully deterministic (the few `Math.random`-derived fields like
`storePath` are now frozen to the fixture).

## Suggested CI use

Two directions — pick whichever is easier for your stack:

1. **JSON → Dioxus (recommended).** Deserialize `crystal-forge.fixtures.json` into your
   Rust structs (serde). If it deserializes with no missing/renamed fields, your types
   already match the design. Then render each view from the fixture and snapshot-assert
   the DOM/text against expected values.
2. **Rendered → JSON.** Scrape the values your Dioxus app renders for a given fixture
   record and assert equality against the corresponding entry here.

Either way, key each assertion by the stable `id` fields (`sys-*`, `eval-*`, build ids,
`CVE-*`, `fl-*`, policy ids, bundle ids) — never by array position.

## Regenerating

Two artifacts stay in lockstep and are both derived from the mock's `data-*.js` modules
(run in load order with `Math.random` seeded to 1337):

- `crystal-forge.fixtures.json` — the canonical golden data (commit this; Dioxus/CI reads it).
- `crystal-forge.fixtures.js` — a `window.__CF_FIXTURES = <same JSON>` wrapper the HTML loads.

Regenerate both whenever the mock data changes, so the file, the wrapper, and the live
example never drift. (Ask the assistant to "regenerate fixtures".)

## Top-level shape

```jsonc
{
  "_meta":        { "generated", "source", "rngSeed", "note" },
  "environments": [ { name, color, dot } ],
  "flakes":       { "registry": [Flake], "commits": { "<flakeId>": [Commit] } },
  "systems":      [System],
  "builds":       { "active": [Build], "history": [Build], "stats": {...}, "workers": [Worker] },
  "evaluations":  { "active": [Eval], "history": [Eval], "stats": {...} },
  "cves":         { "list": [Cve], "stats": {...}, "insights": {...} },
  "policies":     [Policy],
  "compliance":   [Bundle],
  "caches":       [Cache],
  "scanning":     { "configs", "stats", "activity", "history": [ScanRow] },
  "admin":        { "users", "oidcMappings", "roles", "auditLog", "server", "backgroundJobs", "heartbeat" },
  "hardening":    [HardeningService]
}
```

## Entity fields (the ones views assert on)

**System** — `id`, `hostname`, `fqdn`, `environment`, `flake`, `branch`, `commit`,
`commitMessage`, `health` (`healthy|warning|critical|offline|drifted|building|unknown`),
`status`, `statusColor`, `statusChip`, `deploymentPolicy`, `deploymentState`,
`lastHeartbeat`, `heartbeatAge`, `heartbeatIntervalSec`, `heartbeatNextInSec`,
`generation`, `nixosVersion`, `kernel`, `storePath`, `targetStorePath`, `uptime`, `cpu`,
`memGb`, `ipv4`, `ipv6`, `reachability`, `cves {critical,high,medium,low,total}`, `tags[]`,
`stig`, `events[]`.

**Build** — `id`, `system`, `name`, `flake`, `drv`, `commit`, `status`, `meta{label,color,cls}`,
`worker`, `arch`, `totalDerivs`, `builtDerivs`, `cachedDerivs`, `currentPkg`, `queuedAt`,
`dur`, `progress`, `attempts`, `logLines`, `failedPkg`.

**Eval** — `id`, `flake`, `commit`, `branch`, `status`, `meta`, `systemCount`, `policyPass`,
`policyFail`, `queuePos`, `startedAt`, `completedAt`, `dur`, `canCancel`, `canForceCancel`.

**Cve** — `id`, `pkg`, `severity`, `cvss`, `title`, `introducedIn`, `fixedIn`, `fix`,
`ageDays`, `exploited`, `affected[]`, `affectedCount`, `advisoryUrl`, `vector`,
`discoveredAt`, `acceptance`, `justification`, `justifiedBy`, `justifiedAt`.

**Flake** — `id`, `name`, `url`, `branch`, `description`, `environment`, `systemCount`,
`lastSyncAt`, `status`, `latestCommit`, `latestMessage`, `latestAuthor`, `latestAt`,
`totalCommits`. Commit lists live under `flakes.commits["<flakeId>"]`.

**Policy** — `id`, `name`, `category`, `description`, `type`, `rules[]`, `rationale`
(imported STIG policies also carry `severity`, `enabled`, `source`, `evidence[]`).

**Bundle** — `id`, `name`, `framework`, `version`, `description`, `layer`, `owner`,
`lastReview`, `policyIds[]`, `requiredEnvs[]`.

**Cache** — `id`, `name`, `type`, `url`, `region`, `status`, `storage`, `paths`,
`lastPush`, `pushRate`, `environments[]`, `requiresAuth`, `credId`, `createdAt`, `createdBy`.

**ScanRow** — `id`, `hostname`, `flake`, `environment`, `statusColor`, `commits[]`,
`totalConfigs`, `scanned`, `stale`, `needsBuild`, `unscanned`, `currentCrit`, `currentHigh`.

**HardeningService** — `id`, `name`, `score`, `risk`, `riskColor`, `enabled[]`, `missing[]`,
`nixSnippet`, `user`, `notes`.

## Notes

- Relative timestamps (`"4m ago"`, `"yesterday"`) are frozen strings in the mock, not real
  dates. Assert them as opaque strings, or normalize both sides through the same
  relative-time formatter.
- Colors are literal hex/CSS tokens the design uses; assert them if your port reproduces
  the palette, otherwise ignore the `*Color`/`statusChip` fields.
