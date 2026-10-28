---
type: Operator Guide
title: Authentication modes and OIDC configuration examples
description: Shows the server authentication modes (oidc and local in the NixOS module, plus dev for debug builds), an OIDC configuration with claim mapping, and the matching environment variables; open it when configuring server sign-in.
tags:
  - crystal-forge
  - authentication
  - oidc
  - configuration
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T21:00:00-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file README.md at commit 3b23d36f"
    title: Crystal Forge README
---

# Authentication modes and OIDC configuration examples

## Authentication Modes

The server supports three authentication modes, selected by the `AUTH_MODE` environment variable (default `oidc`). The NixOS module sets the variable from `services.crystal-forge.server.auth_mode`. The module accepts **only `local` and `oidc`**. Its default is `local`.

```nix
# Option 1: OIDC (production with an identity provider)
services.crystal-forge.server = {
  auth_mode = "oidc";
  oidc = {
    issuerUrl = "https://keycloak.example.com/realms/crystal-forge";
    clientId = "crystal-forge-web";
    clientSecretFile = "/run/secrets/oidc-client-secret";
    redirectUri = "https://forge.example.com/api/auth/oidc/callback";
  };
};
```

```nix
# Option 2: Local username/password (self-hosted, the module default)
services.crystal-forge.server.auth_mode = "local";
```

**Dev mode is not a NixOS module value.** `services.crystal-forge.server.auth_mode = "dev"` fails the module type check. Dev mode exists only for local development with a **debug build** of the server binary. It uses three fixture users, and a release build refuses to start with it. To use it, set `AUTH_MODE=dev` for a debug build of the server. The fixture-backed UI stack (`run-ui-dev`) does not use it. That stack starts the server in `local` mode with a bootstrap `admin` user. See [Local development workflow](local-development-workflow.md) and [API authentication](../security/api-authentication-and-authorization.md#dev-mode) for the dev login route and the fixture users.

![Local sign-in with username and password](../../screenshots/04-post-register-login.png)

The screenshot shows the `local` mode sign-in form.

## OIDC Configuration Example

```nix
services.crystal-forge.server = {
  auth_mode = "oidc";
  oidc = {
    issuerUrl = "https://keycloak.company.com/realms/prod";
    clientId = "crystal-forge";
    # Use clientSecretFile. A literal clientSecret also exists, but it places the secret in the Nix store.
    clientSecretFile = "/run/secrets/oidc-secret";
    redirectUri = "https://forge.company.com/api/auth/oidc/callback";
    scopes = ["openid" "profile" "email"];
    # Optional: map claims from your provider
    rolesClaim = "groups";           # groups used for role mapping
    preferredUsernameClaim = "preferred_username";
    emailClaim = "email";
    nameClaim = "name";
  };
};
```

## Environment Variables

```bash
# Server
CRYSTAL_FORGE__SERVER__HOST=0.0.0.0
CRYSTAL_FORGE__SERVER__PORT=3000

# Auth
AUTH_MODE=local  # or "oidc" ("dev" works only in debug builds)

# OIDC (when AUTH_MODE=oidc)
CRYSTAL_FORGE_OIDC_ISSUER_URL=https://keycloak.example.com/realms/crystal-forge
CRYSTAL_FORGE_OIDC_CLIENT_ID=crystal-forge-web
CRYSTAL_FORGE_OIDC_CLIENT_SECRET=secret
CRYSTAL_FORGE_OIDC_REDIRECT_URI=https://forge.example.com/api/auth/oidc/callback
```

## Related concepts

- [NixOS module configuration](nixos-module-configuration.md)
- [Server configuration reference](server-configuration-reference.md)
- [Authentication and authorization overview](../security/authentication-and-authorization-overview.md)
- [OIDC role mapping](../security/oidc-role-mapping.md)
- [OIDC provider compatibility validation](../security/oidc-provider-compatibility-validation.md)
- [Session cookies and CSRF](../security/session-cookies-and-csrf.md)
