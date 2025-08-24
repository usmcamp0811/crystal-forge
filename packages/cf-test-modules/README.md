# Crystal Forge Tests (devshell & NixOS VM)

## Quick start (devshell)

```sh
# NixOS: from repo root
nix develop -c pytest -m "smoke or (views and not slow)"  # fast set
nix develop -c cf-test -m systems_status                  # via entrypoint
nix develop -c python -m cf_test.tests.run_tests quick    # script runner
```

## NixOS VM tests

Inside your VM test `machine.succeed` step, call:

```sh
cf-test -m smoke
cf-test -m systems_status
python -m cf_test.tests.run_tests systems-status
```

`CFTestConfig` auto-detects VM env via `NIX_BUILD_TOP` and tries the Postgres
unix socket `/run/postgresql` then falls back to TCP. Env vars: `DB_HOST/PORT/USER/PASSWORD/NAME`. Artifacts land in `/tmp/cf-test-outputs` and are copied to `$out/cf-test-results` when under NixOS tests.

## Adding a new view test

1. Import `CFTestClient` and (optionally) `scenarios`.
2. Use a scenario builder or insert minimal rows.
3. Query your view; assert the expected shape/status.
4. Save evidence with `cf_client.save_artifact(...)`.
5. Cleanup using patterns from the scenario `cleanup`.

## Useful markers

- `@pytest.mark.smoke` quick checks
- `@pytest.mark.views` db view tests
- `@pytest.mark.slow` perf/large scans
- `@pytest.mark.systems_status` (auto-applied to files with “systems_status”)

### Why this fits what you already have

- You already ship a pytest entrypoint and env-driven config (see `tool.pytest.ini_options` and `CFTestConfig`), so these drop in cleanly. :contentReference[oaicite:0]{index=0} :contentReference[oaicite:1]{index=1}
- `conftest.py` verifies DB at session start and registers markers; this doc/structure keeps that flow. :contentReference[oaicite:2]{index=2}
- `run_tests.py` and `test_quick_validation.py` remain useful for quick smoke/structure checks alongside the scenario suite. :contentReference[oaicite:3]{index=3} :contentReference[oaicite:4]{index=4}
- The scenarios mirror the states your current tests already implement (up-to-date, behind, offline, eval-failed) but DRY them up for faster additions. :contentReference[oaicite:5]{index=5}
- Smoke tests remain unchanged. :contentReference[oaicite:6]{index=6}
