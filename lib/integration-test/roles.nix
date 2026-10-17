# VM role modules for native NixOS integration checks.
#
# Topology contract (maximum five VMs per scenario):
#
#   controller - real CF server, PostgreSQL, and server-side workers.
#   services   - Git/upstream fixtures, cache services, local OIDC, TLS,
#                notification receiver.
#   builderA   - real builder with its own Nix store and credentials.
#   builderB   - second real builder; MAY also run a second real agent.
#   targetA    - real bootable NixOS target and agent.
#
# Only `controller` is implemented in the foundation. The other role
# functions are added here by the coordinator when the first scenario that
# needs them is implemented, so that each role is proven by an executed
# scenario before other packets depend on it.
#
# INVARIANT: Role modules use only documented `services.crystal-forge`
# options and the production packages from lib/default.nix. A role MUST NOT
# change application behavior to make a scenario pass.
#
# Shared store: NixOS test VMs read the host /nix/store through a read-only
# mount, but each VM registers only its own closure as valid. A path that
# exists on the host is therefore visible as a file in every VM. Scenarios
# that prove build, publication, or copy MUST use an output that is first
# produced inside the VM (for example, with a per-run nonce) and MUST check
# absence with `assert_store_absent` (driver.py), which tests both validity
# and file presence.
{
  pkgs,
  lib,
  inputs,
  packages,
  browserLib,
}: {
  # Module added to every node. It names the node after its role and pins the
  # tools that the Python driver helpers call.
  common = role: {...}: {
    networking.hostName = role;
    system.stateVersion = lib.mkDefault "26.05";
    environment.systemPackages = [pkgs.curl pkgs.jq];
  };

  # Controller: the production server through the normal NixOS module.
  #
  # Arguments:
  #   browser - add the pinned browser so a scenario can open the production
  #             UI at http://localhost:3000.
  #
  # The module default `server.auth_mode = "local"` is set explicitly so the
  # scenario states the configuration that it claims to test. The colocated
  # builder is disabled because builders are separate roles.
  controller = {browser ? false}: {...}: {
    imports =
      [inputs.self.nixosModules.crystal-forge]
      ++ lib.optional browser browserLib.module;

    virtualisation.memorySize = 4096;
    virtualisation.cores = 4;
    virtualisation.diskSize = 8192;

    networking.firewall.allowedTCPPorts = [3000];

    services.crystal-forge = {
      enable = true;
      local-database = true;
      server = {
        enable = true;
        package = packages.server;
        port = 3000;
        auth_mode = "local";
      };
      build.enable = false;
    };

    environment.systemPackages = [pkgs.postgresql];
  };
}
