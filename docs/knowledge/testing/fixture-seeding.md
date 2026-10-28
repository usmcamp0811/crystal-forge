---
type: Testing Guide
title: Fixture seeding developer guide
description: "Developer guide to fixture mode, where the server seeds its local development database from the golden fixture JSON for run-ui-dev; distinguishes this stack from the Playwright-mocked ui-screenshots check and documents the seeder's current contents."
tags:
  - crystal-forge
  - testing
  - fixtures
  - run-ui-dev
implementation_status: partial
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/fixture-seeding.md at commit 3b23d36f"
    title: "Fixture Seeding — Developer Guide"
---

# Fixture Seeding — Developer Guide

The Crystal Forge server can be started in **fixture mode**: it reads
`docs/design/CrystalForge/fixtures/crystal-forge.fixtures.json` at startup,
seeds the application database, and then runs normally. The regular API handlers
serve data from that database — no mocking, no route interception.

This gives you two things:

1. **`run-ui-dev`** — a single devshell command that starts PostgreSQL, seeds the
   fixture data, starts the server (background), and starts the Dioxus hot-reload
   dev server (foreground). Iterate on the frontend with real populated data.

2. **`nix build .#checks.x86_64-linux.ui-screenshots`** — a non-interactive Nix
   build that serves the production WASM bundle and captures each configured
   route in two themes. Playwright fulfills API requests with fixture JSON. The
   check starts no Crystal Forge server and no database. See
   [Crystal Forge flake checks](flake-checks.md).

---

## Quick start

```bash
# Enter the dev environment
nix develop

# Start everything in one command
run-ui-dev
# → PostgreSQL   localhost:3042
# → API server   http://localhost:3445   (seeded with fixture data)
# → Dioxus UI    http://localhost:8080   (hot-reload)

# Ctrl-C shuts down the server; PostgreSQL keeps running until you stop it.
# To also stop PostgreSQL: db-only down
```

Pass `--dev` to rebuild the server from local source instead of the Nix package:

```bash
run-ui-dev --dev
```

`run-ui-dev` runs **everything in one foreground process** — you do not need a
second terminal or a manual `dx serve`. It handles the two things that make a
bare `dx serve` fail in this repo:

1. **wasm-bindgen version pin** — the project needs wasm-bindgen `0.2.108`, but
   the devshell's `wasm-bindgen-cli` is newer. The script symlinks the pinned
   `0.2.108` binary into `$XDG_DATA_HOME/dioxus/wasm-bindgen/` (exactly like the
   Nix `web-ui` build does) so `dx` uses the right one.
2. **working directory** — `dx serve` must run from `packages/web-ui`, not the
   repo root or `packages/default`.

The `dx and dioxus versions are incompatible` message from `dx` is a **non-fatal
warning** (the Nix build shows it too); it does not stop the build.

### Frontend only (server already running)

If the CF API server is already up (e.g. via `run-ui-dev` in another shell, or
`server-stack-mock`), you can run just the hot-reload frontend:

```bash
run-ui-frontend
```

This pins wasm-bindgen, rebuilds Tailwind, and runs `dx serve` from
`packages/web-ui` against the server on `http://localhost:3445`.

### Why bare `dx serve` fails

```text
Build failed: Incorrect wasm-bindgen-cli version:
project requires version 0.2.108 but version 0.2.121 is installed
```

Use `run-ui-dev` or `run-ui-frontend` instead of calling `dx serve` directly —
they set up the pinned toolchain for you.

---

## How the seeding works

`packages/default/crates/cf-server/src/fixtures/seed.rs` reads the fixture JSON and INSERTs rows
into the application tables in FK-safe order:

| Step | Table(s) | Fixture section |
|------|----------|-----------------|
| 1 | `environments` | `environments[]` |
| 2 | `flakes` | `flakes.registry[]` |
| 3 | `commits` | `flakes.registry[].latest_commit` |
| 4 | `deployment_policies` | `policies[]` |
| 5 | Compliance framework versions and requirements | `compliance[]` |
| 6 | `users` + `user_role_assignments` | `admin.users[]` |
| 7 | `systems` | `systems[]` |
| 8 | `system_states` + `agent_heartbeats` | `systems[]` hardware fields |
| 9 | `system_events` + `pending_system_deployments` | `systems[]` |
| 10 | `cves` + package vulnerabilities and scans | `cves.list[]` |
| 11 | `builders` + `build_jobs` | `builds.active[]`, `builds.history[]`, `builds.workers[]` |
| 12 | Hardening scans and results | `hardening[]` |
| 13 | Setup wizard dismissed for seeded users | all seeded users |

All INSERTs use `ON CONFLICT … DO UPDATE` so re-seeding is idempotent.

The server seeder also reads `caches[]`, `scanning`, and `evaluations`, but
`seed_from_fixture` does not seed those sections at this revision. The seeder
explicitly dismisses the onboarding coach for seeded users.

### Env vars consumed at startup

| Variable | Description |
|----------|-------------|
| `FIXTURE_JSON_PATH` | Absolute path to the fixture JSON. When set, seeding runs after migrations. |
| `AUTH_MODE` | Set to `dev` to enable development login for configured fixture users. |
| `CRYSTAL_FORGE__SERVER__EXECUTION_MODE` | Set to `mock` to skip real nix-eval/build jobs. |
| `RUST_LOG` | `info,crystal_forge::fixtures::seed=debug` shows per-table row counts. |

---

## What is seeded vs not yet implemented

### Seeded by `seed_from_fixture`

- System list, environment list, flake list
- System health (derived from agent heartbeats)
- CVE list and summary stats
- Deployment policies
- Compliance framework versions and requirements
- Builder list
- Build jobs and build history
- System events and pending deployments
- Hardening scans and results
- Users / auth (dev-mode auto-login)

### Fixture sections not consumed by `seed_from_fixture`

The following fixture sections are present in the JSON, but the current server
seeder does not consume them:

| Fixture section | Seeder behavior |
|-----------------|-----------------|
| `caches[]` | Not consumed by `seed_from_fixture`. |
| `scanning` | Not consumed by `seed_from_fixture`. |
| `admin.auditLog` | Not consumed by `seed_from_fixture`. |
| `evaluations` | Not consumed by `seed_from_fixture`. |

An empty panel is not proof that a feature is unwired. It can also mean that the
fixture seeder does not create data for that panel or that the active check
uses mocked API data. Check the relevant check's setup before treating an empty
panel as a product gap.

---

## Adding a new field to the seeder

When the backend implements a new feature that has a corresponding fixture
section, wire it into `seed.rs` following this pattern:

### 1 — Add a struct to parse the fixture JSON

```rust
// In packages/default/crates/cf-server/src/fixtures/seed.rs

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FixtureMyThing {
    id: String,
    name: String,
    some_field: Option<String>,
}
```

Add the field to `FixtureRoot` (or the relevant parent struct):

```rust
struct FixtureRoot {
    // ...existing fields...
    my_things: Vec<FixtureMyThing>,
}
```

### 2 — Write a `seed_my_things` function

```rust
async fn seed_my_things(pool: &PgPool, items: &[FixtureMyThing]) -> Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    tracing::info!("Seeding {} my_things", items.len());
    for item in items {
        sqlx::query(r#"
            INSERT INTO my_things (id, name, some_field)
            VALUES ($1, $2, $3)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                some_field = EXCLUDED.some_field
        "#)
        .bind(&item.id)
        .bind(&item.name)
        .bind(item.some_field.as_deref())
        .execute(pool)
        .await
        .with_context(|| format!("Failed to seed my_thing '{}'", item.id))?;
    }
    Ok(())
}
```

### 3 — Call it from `seed_from_fixture`

Add it in FK-safe order (after any tables it references):

```rust
pub async fn seed_from_fixture(pool: &PgPool, path: &Path) -> Result<()> {
    // ...existing calls...
    seed_my_things(pool, &fixture.my_things).await?;
    Ok(())
}
```

### 4 — Update the table above

Change `🚧 Not yet seeded` → `✅ Seeded` and remove the tracking task reference.

### 5 — Test manually

```bash
run-ui-dev --dev   # rebuilds server from source
```

Open http://localhost:8080 and confirm the panel now shows data.

---

## Adding a new route to the screenshot check

The `ui-screenshots` Nix derivation (`checks/ui-screenshots/default.nix`)
serves the production WASM bundle and captures configured routes with Playwright
API fixtures. It does not start the fixture-seeded server.

To add a new route:

1. Add an entry to the route list built by
   `checks/ui-screenshots/routes.js`:
   ```js
   { path: '/my-new-route', name: 'my-new-route' },
   ```
2. Run `nix build .#ui-screenshots` to capture a screenshot.
3. The output is `result/my-new-route--dark.png` and `result/my-new-route--light.png`.

---

## FAQ

**Q: Does the screenshot check use the fixture-seeded server?**

No. The `ui-screenshots` check uses Playwright route fixtures and serves the
production WASM bundle. `run-ui-dev` is a separate workflow that starts
PostgreSQL, seeds a task-owned local database, and runs the API server.

**Q: Why does the server start so fast if it's running migrations and seeding?**

SQLx migrations are idempotent (skipped if already applied). Seeding uses
`ON CONFLICT DO UPDATE` so a second `run-ui-dev` is as fast as the first.

**Q: Can I point `FIXTURE_JSON_PATH` at a different file?**

Yes. The JSON must contain the required root fields in `FixtureRoot`.
Some fields within those sections are optional. Use the Rust struct definitions
in `packages/default/crates/cf-server/src/fixtures/seed.rs` as the schema.

**Q: How do I reset the database to a clean fixture state?**

```bash
# Stop the server (Ctrl-C in run-ui-dev)
db-only down
db-only up   # fresh PostgreSQL
run-ui-dev   # seeds from scratch
```
