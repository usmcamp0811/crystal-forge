# Title

<!--
Short, outcome-focused title
-->

---

# Problem

<!--
Brief description of the issue or opportunity.
Keep this lightweight.
-->

---

# Desired Outcome

<!--
What should be true if this is completed?
-->

---

# Notes

<!--
Optional context, links, screenshots, or references.
-->

---

# Scope Hint (Optional)

<!--
If obvious, describe rough boundaries.
Not required at Backlog stage.
-->\n\n# Issue Details\n\n- **Issue ID:** 183666508\n- **Issue IID:** 113\n- **Title:** Duplicate `ssh_key_path` in config.toml when server and builder start concurrently\n- **State:** opened\n- **Labels:** type::bug\n- **Created by:** Matt\n- **Created at:** 2026-02-08T15:13:40.827Z\n- **Updated at:** 2026-02-08T15:13:40.827Z\n\n# Description\n\n  ## Description

  When both `crystal-forge-server` and `crystal-forge-builder` services are enabled and start concurrently, the generated
  `config.toml` file contains a duplicate `ssh_key_path` entry in the `[auth]` section, causing the service to fail with a TOML parse
   error.

  ## Error

  Error: loading configuration
  Caused by:
      TOML parse error at line 3, column 1
        |
      3 | ssh_key_path = "/var/lib/crystal-forge/.ssh/id_ed25519"
        | ^
      duplicate key ssh_key_path in table auth
       in config.toml

  ## Root Cause

  The issue occurs in the `makeConfigScript` function (around line 252-268 in the NixOS module). When `cfg.auth.ssh_key_path == null`
   and both server and builder are enabled:

  1. Both services run `preStart` scripts that call the same config generation script
  2. Both scripts check if the SSH key exists, and if not, generate it
  3. Both scripts then run: `${pkgs.gnused}/bin/sed -i '/\[auth\]/a ssh_key_path = "/var/lib/crystal-forge/.ssh/id_ed25519"'
  "$generatedConfigPath"`
  4. Due to the race condition, both sed commands execute, adding `ssh_key_path` twice

  ## Affected Configuration

  ```nix
  services.crystal-forge = {
    enable = true;
    server.enable = true;
    build.enable = true;
    auth.ssh_key_path = null; # Auto-generate SSH key
  };

  Proposed Fix

  Make the sed command idempotent by checking if ssh_key_path already exists before adding it:

  # Instead of:
  ${pkgs.gnused}/bin/sed -i '/\[auth\]/a ssh_key_path = "/var/lib/crystal-forge/.ssh/id_ed25519"' "$generatedConfigPath"

  # Use:
  if ! grep -q "ssh_key_path" "$generatedConfigPath"; then
    ${pkgs.gnused}/bin/sed -i '/\[auth\]/a ssh_key_path = "/var/lib/crystal-forge/.ssh/id_ed25519"' "$generatedConfigPath"
  fi

  Workaround

  Until fixed, add explicit systemd ordering to prevent the race condition:

  systemd.services.crystal-forge-builder = {
    after = [ "crystal-forge-server.service" ];
  };

  Environment

  - NixOS version: 25.11
  - Crystal Forge flake: gitlab:crystal-forge/crystal-forge/112-not-making-new-generations-2
  - Both server and builder enabled on the same host\n\n# Weight\n\n0\n\n# Time Stats\n- Time Estimate: 0\n- Total Time Spent: 0\n