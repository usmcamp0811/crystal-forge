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

## Private credential permissions and upgrades

The current module manages named state, work and cache directories without
recursive permission or ownership changes. NixOS activation and server or
builder startup preserve the owner, group, mode and contents of existing
caller-provided files under `/var/lib/crystal-forge`, including `.cache` and
`.config`. Directory modes do not define private-file modes. Provision private
SSH keys for the service identity with mode `0600`. Keep every parent directory
traversable by that identity.

Configured external paths and symlinks remain supported. Their existing secret
manager or operator owns provisioning, access permissions and replacement. Do
not put private key contents in Nix expressions, the Nix store, logs or review
artifacts. Keep SSH host verification and cache signature verification enabled.

The module still owns these explicit file lifecycles:

- It regenerates `config.toml` with mode `0600`.
- With `auth.ssh_key_path = null`, it creates the default Git SSH key if absent
  and restricts the writable default private key to mode `0600`.
- With `build.api_key_file = null`, it creates the default builder API key if
  absent and sets its owner to `crystal-forge` and mode to `0600`. It retains
  existing key contents. It manages the corresponding public key separately.
- It does not normalize caller-provided SSH, builder API or agent private keys.
  The root agent uses its configured key and separate agent state directory.

### Upgrade: prevent recurrence and inspect affected files

Earlier modules used a recursive `Z /var/lib/crystal-forge/ 0755` tmpfiles rule.
Server and builder startup also recursively changed `.cache` and `.config`.
Those operations could widen private-file modes, change ownership and make
OpenSSH refuse a valid key. The current correction prevents those operations;
it cannot infer or restore the intended metadata of arbitrary affected files.
An older generation can still apply its old rules if activated again.

Use a bounded credential inventory from the host configuration and the existing
secret manager. For each configured credential, inspect the path, symlink target,
owner, group and mode. For example, run these commands with permission to inspect
the one selected path; do not print the file contents:

```sh
stat -c '%n %u:%g %a %F' /var/lib/crystal-forge/cache-ssh
stat -Lc '%n %u:%g %a %F' /var/lib/crystal-forge/cache-ssh
readlink /var/lib/crystal-forge/cache-ssh
```

`readlink` returns nonzero for a regular file. Compare the metadata with the
credential's provisioning contract. If the file was affected, restore the
intended owner and permissions through the existing secret manager or its
documented operator procedure after installing the corrected module. Do not
apply a recursive chmod/chown or assume that every file has the same owner or
mode. Recheck after activation and the relevant service restart. Test the
configured authenticated cache as the actual service identity, using its trusted
host key and explicit identity with agent/password fallback disabled.

Permission restoration does not prove that nobody read the key while access
was wider. Assess the possible exposure using the affected interval, directory
traversal permissions, local principals and available audit evidence. Replace
and revoke the credential through the existing lifecycle when that assessment
requires it. A widened mode alone does not establish actual compromise. The
module does not automatically rotate or delete keys or change registered
identities.

## Bulk evaluator resources

Before tuning build resources, configure bulk evaluation separately. The
adaptive evaluator contract defaults `server.eval_max_memory_mb`
to `null`; the generator omits that key from TOML. A positive integer
remains a fixed per-worker MiB threshold. With two workers, `12288` preserves
a 24576 MiB configured product. `build.systemd_memory_max` in the quick-start
example controls build execution, not the bulk evaluator's sizing policy.

See [Bulk evaluator memory planning and timeouts](../evaluation/bulk-evaluator-resource-planning.md)
for runtime host/cgroup detection, the 4096 MiB reserve,
85-percent policy, and independent 900-second idle / 3600-second overall
deadlines. An absent memory override enables automatic sizing on upgrade;
set an explicit `4096` if the previous fixed threshold is required.

The working boundary includes independently detected finite `memory.high`
and `memory.max` values across visible ancestors. `memory.high` throttles
reclaim; it is not an OOM-kill limit. No new service limit is installed.
See [Adaptive bulk recovery](../evaluation/bulk-evaluator-adaptive-recovery.md)
for isolated retry caps, shared deadlines, pressure diagnostics, and retained
current-attempt outcomes after a resource-terminal failure.

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
