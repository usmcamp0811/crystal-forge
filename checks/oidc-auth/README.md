# OIDC Authentication Check

This check proves the OIDC login path end to end against a real identity
provider. It boots two NixOS VMs: one runs Keycloak with a pre-imported
`crystal-forge` realm, the other runs the Crystal Forge server configured
with `auth_mode = "oidc"` and pointed at that realm.

## What it verifies

- The Keycloak OIDC discovery document is reachable, both directly and from
  the server node, and contains the required endpoints.
- The Resource Owner Password Credentials token exchange succeeds and the
  returned access token, ID token, and refresh token are present.
- Realm role claims are mapped into the `roles` claim (not `groups`) and
  contain the expected `admin` role.
- The server reports `auth_mode: "oidc"` and an unauthenticated `whoami`
  before login.
- `/api/auth/oidc/login` redirects to Keycloak.
- The `users`, `user_sessions`, and `external_identities` tables exist after
  migration.

## Why it is a separate check

OIDC is a distinct authentication mode from the local-auth path exercised by
the `integration` check. Standing up Keycloak, importing a realm, and
performing a full authorization-code-adjacent token exchange only makes
sense as its own VM topology; folding it into `integration` would make that
check depend on Keycloak for every run, including runs that only care about
local auth.

## Run it

```sh
nix build .#checks.x86_64-linux.oidc-auth --print-build-logs
```

This boots two VMs (`keycloak`, `server`) with a 10-minute global timeout.
It performs no interactive browser automation; the OIDC flow is exercised
directly against Keycloak and the server's HTTP API, not through a browser.

## Out of scope

- The web UI is not involved. The server here uses the core build
  (`cf-server-core-drv`), the same variant used by `integration`, to avoid
  rebuilding this check on every Dioxus change.
- The builder is disabled (`build.enable = false`); this check does not
  exercise builds, evaluation, or flake polling.
- Browser-driven OIDC login (clicking through the Keycloak login form) is not
  covered here; this check drives the token endpoint directly.

## CI

Part of the `.gitlab-ci.yml` `flake-check` matrix (`CHECK_NAME: oidc-auth`),
so it runs on every merge request and on `main`.

## Related files

- `realm-crystal-forge.json` — the Keycloak realm export imported by the
  `keycloak-realm-import` systemd service before the test script starts.
