---
type: Operator Guide
title: NixOS module configuration quick start
description: Shows a complete example of the crystal-forge NixOS module (database, server, builder, agent, watched flakes, systems, binary cache) and how to enable the STIG compliance modules; open it when setting up a deployment.
tags:
  - crystal-forge
  - nixos
  - configuration
  - stig
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T08:25:07-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file README.md at commit 3b23d36f"
    title: Crystal Forge README
---

# NixOS module configuration quick start

> **Status:** partial. This concept holds the `Quick Start` introduction, the `NixOS Module Configuration` section, and the `STIG Compliance Modules` section of the repository `README.md`. The option names exist in `modules/nixos/crystal-forge/default.nix` (`database`, `server`, `build`, `client`, `flakes.watched`, `systems`, `cache`), but the example values and the `30+` STIG count are verification candidates. The STIG module directories are under `modules/nixos/stig-modules/` and the STIG levels under `modules/nixos/stig/`. The options under `crystal-forge.stig` in the second example are set by the preset modules in `modules/nixos/stig/*/default.nix`; the exact control names are unchecked. The README text does not mention the `crystal-forge.stig-presets.*` options.

**New to Crystal Forge?** See the **[Onboarding Guide](onboarding-first-time-setup-prerequisites.md)** for a complete step-by-step walkthrough using the built-in guided setup coach.

## NixOS Module Configuration

```nix
{
  services.crystal-forge = {
    enable = true;

    # Database
    database = {
      host = "/run/postgresql";
      user = "crystal_forge";
      name = "crystal_forge";
      passwordFile = "/run/secrets/db_password";
    };

    # Server (API + Web UI)
    server = {
      enable = true;
      host = "0.0.0.0";
      port = 3000;
      auth_mode = "local";  # or "oidc" for production
    };

    # Builder
    build = {
      enable = true;
      max_concurrent_derivations = 4;
      max_jobs = 4;
      cores_per_job = 4;
      systemd_memory_max = "16G";
    };

    # Agent (on each monitored system)
    client = {
      enable = true;
      server_host = "crystal-forge.example.com";
      server_port = 3000;
      private_key = "/var/lib/crystal-forge/host.key";
    };

    # Flakes to monitor
    flakes.watched = [
      {
        name = "infrastructure";
        repo_url = "git+ssh://git@gitlab.com/company/nixos-configs";
        auto_poll = true;
        initial_commit_depth = 10;
      }
    ];

    # Systems to track
    systems = [
      {
        hostname = "server1";
        public_key = "base64-encoded-ed25519-pubkey";
        environment = "production";
        flake_name = "infrastructure";
        deployment_policy = "manual";
      }
    ];

    # Binary cache
    cache = {
      cache_type = "S3";
      push_after_build = true;
      push_to = "s3://my-bucket?region=us-east-1";
    };
  };
}
```

## STIG Compliance Modules

Crystal Forge provides 30+ NixOS-native STIG implementations:

```nix
# In your flake's nixosModule:
inputs.crystal-forge.nixosModules.crystal-forge

# Enable specific controls
crystal-forge.stig = {
  banner.enable = true;
  # Disable with justification
  account_expiry = {
    enable = false;
    justification = ["Not applicable in development environment"];
  };
};
```

## Related concepts

- [Authentication modes and OIDC configuration examples](authentication-modes-and-oidc-configuration.md)
- [Server configuration reference](server-configuration-reference.md)
- [Onboarding: first-time setup prerequisites](onboarding-first-time-setup-prerequisites.md)
- [STIG modules](../compliance/stig-modules.md)
- [Development environment commands](development-environment-commands.md)
