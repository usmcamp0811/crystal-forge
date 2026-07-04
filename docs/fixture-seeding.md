# Fixture Seeding — Developer Guide

The Crystal Forge server can be started in **fixture mode**: it reads
`docs/design/CrystalForge/fixtures/crystal-forge.fixtures.json` at startup,
seeds the application database, and then runs normally. The regular API handlers
serve data from that database — no mocking, no route interception.

This gives you two things:

1. **`run-ui-dev`** — a single devshell command that starts PostgreSQL, seeds the
   fixture data, starts the server (background), and starts the Dioxus hot-reload
   dev server (foreground). Iterate on the frontend with real populated data.

2. **`nix build .#ui-screenshots`** — a non-interactive Nix build that starts the
   same stack headlessly and captures screenshots of every route.  Use this to
   compare against the design targets.

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

---

## How the seeding works

`packages/default/src/fixtures/seed.rs` reads the fixture JSON and INSERTs rows
into the application tables in FK-safe order:

| Step | Table(s) | Fixture section |
|------|----------|-----------------|
| 1 | `environments` | `environments[]` |
| 2 | `flakes` | `flakes.registry[]` |
| 3 | `commits` | `flakes.registry[].latest_commit` |
| 4 | `deployment_policies` | `policies[]` |
| 5 | `users` + `user_role_assignments` | `admin.users[]` |
| 6 | `systems` | `systems[]` |
| 7 | `system_states` + `agent_heartbeats` | `systems[]` hardware fields |
| 8 | `cves` | `cves.list[]` |
| 9 | `builders` | `builds.workers[]` |

All INSERTs use `ON CONFLICT … DO UPDATE` so re-seeding is idempotent.

### Env vars consumed at startup

| Variable | Description |
|----------|-------------|
| `FIXTURE_JSON_PATH` | Absolute path to the fixture JSON. When set, seeding runs after migrations. |
| `AUTH_MODE` | Set to `dev` for passwordless local auth (auto-login as fixture user). |
| `CRYSTAL_FORGE__SERVER__EXECUTION_MODE` | Set to `mock` to skip real nix-eval/build jobs. |
| `RUST_LOG` | `info,crystal_forge::fixtures::seed=debug` shows per-table row counts. |

---

## What is seeded vs not yet implemented

### ✅ Seeded (works out of the box)

- System list, environment list, flake list
- System health (derived from agent heartbeats)
- CVE list and summary stats
- Deployment policies
- Builder list
- Users / auth (dev-mode auto-login)

### 🚧 Not yet seeded — shows empty / placeholder

The following fixture sections are present in the JSON but have no seeding
code yet, because the backing database tables require a `derivations` FK
(a compiled NixOS derivation path) that doesn't naturally come from the fixture.

| Fixture section | What you'll see | Tracking task |
|-----------------|-----------------|---------------|
| `builds.active` / `builds.history` | Build queue empty | TASK-380 |
| `hardening[]` | Hardening summary zeroed | TASK-381 |
| `compliance[]` | Compliance bundles empty | TASK-382 |
| `caches[]` | Cache destination list empty | TASK-382 |
| `scanning` | Scanning stats zeroed | TASK-382 |
| `admin.auditLog` | Audit log empty | TASK-382 |
| `evaluations` | Eval queue empty | TASK-380 |

These are **intentional gaps** — they highlight unimplemented backend or seeding
work. When you see a blank panel, that's the check telling you "this isn't wired
yet."

---

## Adding a new field to the seeder

When the backend implements a new feature that has a corresponding fixture
section, wire it into `seed.rs` following this pattern:

### 1 — Add a struct to parse the fixture JSON

```rust
// In packages/default/src/fixtures/seed.rs

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
captures a screenshot of each Dioxus route against the fixture-seeded server.

To add a new route:

1. Add an entry to `checks/ui-screenshots/capture.js` (the `ROUTES` array):
   ```js
   { path: '/my-new-route', name: 'my-new-route' },
   ```
2. Run `nix build .#ui-screenshots` to capture a screenshot.
3. The output is `result/my-new-route--dark.png` and `result/my-new-route--light.png`.

---

## FAQ

**Q: Why not mock the API at the Playwright/HTTP layer?**

We tried it — Playwright route handlers run in LIFO order and a catch-all
registered last always wins, causing every request to return `{}`. Seeding the
database is simpler, deterministic, and tests the real handlers.

**Q: Why does the server start so fast if it's running migrations and seeding?**

SQLx migrations are idempotent (skipped if already applied). Seeding uses
`ON CONFLICT DO UPDATE` so a second `run-ui-dev` is as fast as the first.

**Q: Can I point `FIXTURE_JSON_PATH` at a different file?**

Yes. Any JSON that matches the `FixtureRoot` struct in `seed.rs` works.
All fields are `Option<…>` so a minimal stub with just a few systems is fine.

**Q: How do I reset the database to a clean fixture state?**

```bash
# Stop the server (Ctrl-C in run-ui-dev)
db-only down
db-only up   # fresh PostgreSQL
run-ui-dev   # seeds from scratch
```
