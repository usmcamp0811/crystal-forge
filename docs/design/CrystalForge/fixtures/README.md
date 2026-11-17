# Crystal Forge — golden fixtures

`crystal-forge.fixtures.json` preserves the **canonical, deterministic snapshot**
from the former split design runtime. Existing server seeding, UI screenshot,
and integration tooling still consume this JSON. Keep it in place until those
consumers are migrated separately. The current design runtime is
[`../crystal-forge.html`](../crystal-forge.html), with its own inline fixture data.

- **Deterministic** — generated with a seeded RNG (`_meta.rngSeed = 1337`), so every
  regeneration produces byte-identical output. Safe to commit and diff in CI.
- **Complete** — 35 systems, 6 active + 40 historical builds, 4 active + 50 historical
  evaluations, 48 CVEs, 5 flakes (with per-flake commit lists), 13 policies, 4 compliance
  bundles, 5 caches, per-system scan history, and the admin/server registries.

## Runtime and fixture ownership

The single HTML embeds its scripts, styles, and fixture data. Editing this JSON
does not change the design on reload. The former generated JavaScript wrapper
and split mock modules have been removed. Do not regenerate them.

`system-cves-sc1-check.js` remains an independent browser interaction helper.
It opens the single HTML and uses its inline `SC1_CVE_DESIGN` states through
the existing `sc1` query parameter and browser events.

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

## Updating

Changes to this golden JSON require review of its seeding and test consumers.
Do not replace it with a new export's inline fixtures as part of runtime cleanup.

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
