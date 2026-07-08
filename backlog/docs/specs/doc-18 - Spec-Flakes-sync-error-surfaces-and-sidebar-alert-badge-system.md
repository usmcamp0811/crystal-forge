---
id: doc-18
title: 'Spec: Flakes sync-error surfaces and sidebar alert badge system'
type: specification
created_date: '2026-07-08 07:25'
tags:
  - flakes
  - sidebar
  - alerts
  - design-parity
  - web-ui
  - backend
---
# Spec: Flakes sync-error surfaces + sidebar alert/badge system

Implementation guide for the companion backlog task. Follow top to bottom. Every step names the exact file and pattern to copy. Do not improvise new architecture.

## 0. Ground truth — read these files FIRST

Design reference:
- `docs/design/CrystalForge/components/FlakesView.jsx` — attention flash hook usage (line 11), sync-error banner in tray (lines 168-184: "Sync failed" head, `$ nix flake metadata {url}` + error text in a `pre`, last-good-commit + remote meta, Retry sync button), `FlakeSyncChip` (line ~496: synced/syncing/error chip with errorMsg tooltip), table row error flash (line ~520), card error callout (lines ~570-595).
- `docs/design/CrystalForge/components/Shell.jsx` — THE sidebar badge system: `useAttentionFlash`/`acknowledgeView`/`useAcknowledgedViews` (lines 167-200), badge count derivations (lines 225-262), `NAV`/`NAV_OPS`/`NAV_COMPLIANCE` badge fields with tooltips (lines 239-276), `NavItem` rendering `nav-count`/`nav-count-alert` and hiding acknowledged attention badges (lines 284-304).
- `docs/design/CrystalForge/styles.css` — `.fl-sync-error*`, `.nav-count`, `.nav-count-alert`, `.attention-flash` styles.

Current implementation:
- `packages/default/src/flake/commits.rs` — `sync_commits_for_flake` (line ~292) and the background poller call site (line ~258).
- `packages/default/src/handlers/api/flakes.rs` — `sync_flake_handler` (~1899), `sync_all_flakes_handler` (~1828), plus a third call site (~1746, ~2007). Sync errors are currently returned to the caller but NEVER persisted.
- `packages/default/src/queries/flakes.rs` — `list_flake_registry` (~163), `FlakeRegistryItem`.
- `packages/web-ui/src/components/layout/sidebar.rs` — `SidebarNav` (~64), `NavLink` (~778).
- `packages/web-ui/src/views/flakes_list.rs` — flakes view (NOTE: >5700 lines; ALL new UI pieces go in `packages/web-ui/src/components/flake/`, not this file).
- `packages/web-ui/assets/app.css` — `.nav-count` exists (~1933). Verify `.nav-count-alert`, `.attention-flash`, `.fl-sync-error*` exist; port from design styles.css if missing.

## 1. Database migration (NEW file — never edit existing migrations)

NOTE: `0156_deployment_progress_tracking.sql` is reserved by TASK-384. Use the next free number at implementation time (assume `0157_flake_sync_status.sql`):

```sql
ALTER TABLE flakes
    ADD COLUMN IF NOT EXISTS sync_status text NOT NULL DEFAULT 'unknown',
    ADD COLUMN IF NOT EXISTS last_sync_at timestamptz,
    ADD COLUMN IF NOT EXISTS last_sync_error text;

ALTER TABLE flakes
    ADD CONSTRAINT flakes_sync_status_check
    CHECK (sync_status IN ('unknown', 'synced', 'syncing', 'error'));
```

Semantics: `syncing` set when a sync starts; `synced` + `last_sync_at=now()` + `last_sync_error=NULL` on success; `error` + `last_sync_at=now()` + `last_sync_error=<error text>` on failure.

## 2. Backend: record real sync outcomes

### 2.1 One recording wrapper, used everywhere
Add `pub async fn sync_flake_recorded(pool, flake_id, repo_url, branch) -> Result<usize>` in `packages/default/src/flake/commits.rs` that:
1. `UPDATE flakes SET sync_status='syncing' WHERE id=$1`
2. calls the existing `sync_commits_for_flake`
3. on Ok: `UPDATE flakes SET sync_status='synced', last_sync_at=now(), last_sync_error=NULL WHERE id=$1`
4. on Err: `UPDATE flakes SET sync_status='error', last_sync_at=now(), last_sync_error=$2 WHERE id=$1` (truncate error text to 4000 chars), then propagate the error.

Convert ALL FOUR call sites to use it: `flake/commits.rs` ~258 (poller), `handlers/api/flakes.rs` ~1746, ~1855 (sync-all), ~2007 (single sync). Status updates are best-effort: a failed status write logs a warning and never masks the sync result.

### 2.2 Expose in the flakes API
Add `sync_status: String`, `last_sync_at: Option<DateTime<Utc>>`, `last_sync_error: Option<String>` to `FlakeRegistryItem` (queries/flakes.rs) and any single-flake DTO the registry view consumes (`packages/default/src/api/models.rs`). Regenerate sqlx metadata.

## 3. Backend: navigation badge aggregate endpoint

`GET /api/v1/navigation/badges` (route in `bin/server.rs`; handler in a NEW file `packages/default/src/handlers/api/navigation.rs`, registered in `handlers/api/mod.rs`; auth = same guard as other authenticated list endpoints). Response DTO in `api/models.rs`:

```rust
pub struct NavigationBadges {
    pub systems_attention: i64,      // systems whose UI health is critical or offline
    pub systems_total: i64,
    pub flakes_errored: i64,         // sync_status='error' AND deleted_at IS NULL
    pub flakes_total: i64,
    pub environments_attention: i64, // environments containing >=1 attention system
    pub environments_total: i64,
    pub builds_failed_24h: i64,      // failed build derivations, last 24h
    pub evals_failed_24h: i64,       // failed commit evaluations, last 24h
    pub cves_critical: i64,          // open critical CVEs fleet-wide
}
```

Implementation rules:
- One query fn per count in a NEW `packages/default/src/queries/navigation.rs`, each `sqlx::query_as`/`query_scalar`, cheap (COUNT with indexes).
- systems_attention MUST reuse the SAME health semantics the Systems list uses (find the existing status/offline derivation used by `list_systems` / `view_system_deployment_status` + heartbeat staleness; mirror it in SQL — do not invent new thresholds).
- cves_critical MUST reuse the same query logic as the existing `/api/v1/cves/stats` fleet stats (extract/share the query, do not duplicate constants).
- builds_failed_24h / evals_failed_24h: failed within `now() - interval '24 hours'` using the same status columns Builds/Evals views read.
- Unit/DB tests: one `#[ignore]` DB test seeding a known fixture set and asserting each count.

## 4. Web UI: sidebar badges + acknowledgment + attention flash

### 4.1 DTO + client
Mirror `NavigationBadges` in `packages/web-ui/src/api/models.rs` (`#[serde(default)]` on all fields); add `get_navigation_badges()` to `api/client.rs` following the existing fetch pattern.

### 4.2 Shared alert state (NEW module `packages/web-ui/src/alerts/mod.rs`)
Pure, unit-testable core mirroring Shell.jsx lines 167-200:
- `AlertState { acknowledged: HashSet<String>, flashed: HashSet<String> }` held in a Dioxus `GlobalSignal` (or context provided by `AppShell` — follow whichever global-state pattern the app already uses, e.g. how auth/session state is shared).
- `acknowledge(view_key)` — hides that view's attention badge for this page load.
- `should_flash(view_key, has_attention) -> bool` — true exactly once per page load per view while attention is active (consumes `flashed`).
- Badge visibility rule (pure fn, unit tested): show when `count > 0 && !(attention && acknowledged.contains(key))`.

### 4.3 SidebarNav integration (`components/layout/sidebar.rs`)
- One `use_resource` fetching `get_navigation_badges()`, re-polled every 30s (same polling pattern used elsewhere; do NOT poll per-NavLink).
- Extend `NavLink` with optional `count: Option<i64>`, `attention: bool`, `count_title: Option<String>`, `view_key: &'static str`; render `span.nav-count` (+ `.nav-count-alert` when attention) with the tooltip `title` exactly per Shell.jsx line 301.
- Badge mapping (keys must match the alert-state keys used by views): systems→systems_attention (tooltip "N of M systems need attention (critical or offline)"), flakes→flakes_errored ("N of M flakes failing to sync" / "M flakes tracked"), environments→environments_attention, builds→builds_failed_24h ("N failed build(s) in the last 24h"), evals→evals_failed_24h, cves→cves_critical ("N critical CVEs open across the fleet"). Dashboard/Scanning/Policies/Compliance/Builders/Caches/Admin get no badge.
- Rail (collapsed) mode: badge still renders per existing `.sidebar.rail .nav-item .nav-count` CSS.

### 4.4 Attention flash in views
Port `.attention-flash` CSS from design styles.css into `packages/web-ui/assets/app.css` if missing. Wire per view (each = add `should_flash` check on mount + conditional class on alerting rows; acknowledge on arrival):
- Flakes (`views/flakes_list.rs`): rows/cards with `sync_status == "error"` get `attention-flash` once; view acks `flakes` on mount.
- Systems (`views/systems_list.rs`): critical/offline rows/cards flash; acks `systems` on mount.
- Environments: environment cards containing attention systems flash; acks `environments`.
- Builds (`views/builds.rs`): failed rows flash and the badge is acknowledged ONLY when the tab containing failures is opened (Shell.jsx comment lines 168-171 — completed/history tab), not on view mount.
- Evaluations (`views/evaluations.rs`): same tab-scoped rule as Builds.
- CVEs (`views/cves.rs`): critical rows flash; acks `cves` on mount.

## 5. Web UI: Flakes sync-error surfaces (all real data)

All new components in `packages/web-ui/src/components/flake/` (flakes_list.rs is over the module size limit — do not grow it with new component bodies):
1. `FlakeSyncChip` — synced (green) / syncing (blue) / error (red) chip with dot, `title` = last_sync_error (design FlakesView.jsx ~496). Unknown status renders the neutral chip.
2. `FlakeSyncErrorBanner` — the tray banner per design lines 168-184: warn icon + "Sync failed" + relative `last_sync_at` + "Retry sync" button (calls the existing single-flake sync endpoint; on completion refetch the registry so status/chip/banner update) + `pre.fl-sync-error-msg` rendering `$ nix flake metadata {url}\nerror: {last_sync_error}` + meta row (last good commit = latest known commit sha, remote = repo url).
3. Card error callout: when `sync_status == "error"`, card shows the error snippet per design (~570-595).
4. Table: status column uses `FlakeSyncChip`; error rows get attention-flash treatment per §4.4.
5. Page subtitle "N tracked · M systems · K synced" must count `synced` from the new real field.

## 6. Fixtures + web-ui check (screenshots MANDATORY)

- `docs/design/CrystalForge/fixtures/crystal-forge.fixtures.json` + `packages/default/src/fixtures/seed.rs`: set one seeded flake to `sync_status='error'` with a realistic `last_sync_error` (e.g. `error: unable to download 'https://…': HTTP error 403` multi-line) and `last_sync_at` relative-to-now; at least one flake `synced` and one `unknown`.
- `checks/web-ui/coverage-manifest.json` steps: (1) sidebar renders with badge counts — assert the flakes badge shows the errored count and the CVEs badge shows the critical count, screenshot; (2) open /flakes — assert the error chip and error text visible, assert an `attention-flash` class appears on the errored row on first visit, screenshot; (3) open the errored flake's tray — assert "Sync failed" banner text + the seeded error message renders, screenshot; (4) navigate away and back to /flakes — assert the sidebar flakes badge is hidden after acknowledgment while the errored chip still shows in the view.

## 7. Tests (minimum)

- Server unit/DB: `sync_flake_recorded` writes syncing→synced and syncing→error transitions (DB test, `#[ignore]` live-db pattern); badge count queries against seeded fixtures; flakes API returns the new fields.
- UI unit (native target): badge visibility rule; `should_flash` once-per-load behavior; sync-chip status mapping.
- SQLX: `db-only up` + `cargo sqlx prepare` (devshell only).

## 8. Verification (from `nix develop`)

```
cd packages/default && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test
cd packages/web-ui  && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test
# db-only up ; cargo sqlx prepare
nix build .#packages.x86_64-linux.server --no-link
nix build .#packages.x86_64-linux.web-ui --no-link
nix build .#checks.x86_64-linux.web-ui --no-link
nix flake check --keep-going    # migration + server surface => required
```

MR MUST attach the web-ui check screenshots (sidebar badges, flakes error chip/callout, tray Sync failed banner) via GitLab uploads — never committed.

## 9. Out of scope

- The all-views visual drift audit (companion audit task owns it).
- Notification bell/topbar notifications (design Shell.jsx lines ~360-400) — separate feature.
- Flake environment span data (TASK-357.1) and auto-sync interval persistence (TASK-357.2) — remain separate tasks.
- Any redesign of the sync mechanics themselves (webhook/poller cadence unchanged).
