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
  at: 2026-10-04T21:00:00-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file README.md at commit 3b23d36f"
    title: Crystal Forge README
---

# NixOS module configuration quick start

> **Status:** partial. The option names in the first example exist in `modules/nixos/crystal-forge/default.nix` (`database`, `server`, `build`, `client`, `flakes.watched`, `systems`, `cache`). The example values are illustrative. The STIG section below uses the current control and preset names.

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

Crystal Forge exports 25 NixOS-native STIG control modules and four presets (`high`, `medium`, `low`, `off`):

```nix
# In your flake: import the control modules you need and one preset
inputs.crystal-forge.nixosModules.crystal-forge
inputs.crystal-forge.nixosModules."stig-modules/environment/login"
inputs.crystal-forge.nixosModules."stig-modules/environment/account"
# ... every control that your preset sets (see the STIG modules guide)
inputs.crystal-forge.nixosModules."stig/medium"

# Choose a preset
crystal-forge.stig-presets.medium.enable = true;

# Adjust one control. Presets set every control, so override with mkForce.
crystal-forge.stig.account = {
  enable = lib.mkForce false;
  justification = ["Not applicable in development environment"];
};
```

Each preset sets all 25 controls, so import every control module when you use a preset. The control names are listed in [STIG modules](../compliance/stig-modules.md).

## Related concepts

- [Authentication modes and OIDC configuration examples](authentication-modes-and-oidc-configuration.md)
- [Server configuration reference](server-configuration-reference.md)
- [Onboarding: first-time setup prerequisites](onboarding-first-time-setup-prerequisites.md)
- [STIG modules](../compliance/stig-modules.md)
- [Development environment commands](development-environment-commands.md)
