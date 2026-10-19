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
# Roles expose only capabilities required by implemented scenario bodies.
# Defining a constructor does not establish passing runtime evidence.
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

  # Local Git and authenticated Nix-store publication. Each builder generates
  # its own SSH key at runtime; the driver installs only public keys here.
  services = {}: {...}: {
    virtualisation.memorySize = 1024;
    virtualisation.diskSize = 8192;
    services.nginx = {
      enable = true;
      virtualHosts.git = {
        default = true;
        root = "/srv/git";
      };
    };
    # CF uses shallow clones; a static/dumb HTTP Git server cannot serve them.
    # Reuse the NixOS cgit smart-HTTP backend used by the repository Git helper.
    services.cgit.delivery = {
      enable = true;
      scanPath = "/srv/git";
      gitHttpBackend.checkExportOkFiles = false;
      nginx.virtualHost = "git";
      user = "git";
      group = "git";
    };
    users.groups.git = {};
    users.users.git = {
      isSystemUser = true;
      group = "git";
      home = "/srv/git";
    };
    services.openssh = {
      enable = true;
      settings.PasswordAuthentication = false;
      settings.AuthorizedKeysFile = ".ssh/authorized_keys";
    };
    networking.firewall.allowedTCPPorts = [22 80];
    users.groups.cache = {};
    users.users.cache = {
      isNormalUser = true;
      group = "cache";
      home = "/var/lib/cache";
      createHome = true;
    };
    nix.settings = {
      trusted-users = ["root" "cache"];
      substituters = lib.mkForce [];
      builders = lib.mkForce [];
      fallback = false;
    };
    environment.systemPackages = [pkgs.git pkgs.openssh];
    systemd.tmpfiles.rules = ["d /srv/git 0755 root root -"];
  };

  # A production API-only builder. The driver registers its fresh public key
  # and checks output absence before explicitly starting the normal unit.
  builder = {name, strategy ? "server_derivation"}: {...}: {
    imports = [inputs.self.nixosModules.crystal-forge];
    networking.hostName = name;
    virtualisation.memorySize = 2048;
    virtualisation.cores = 2;
    virtualisation.diskSize = 12288;
    services.crystal-forge = {
      enable = true;
      local-database = false;
      server.enable = false;
      build = {
        enable = true;
        package = packages.builder;
        api_mode = true;
        server_url = "http://controller:3000";
        supported_execution_strategies = [strategy];
        use_substitutes = false;
        offline = true;
        sandbox = true;
        max_concurrent_derivations = 1;
        max_jobs = 1;
        cores_per_job = 1;
        cve_scanning_enabled = false;
      };
    };
    systemd.services.crystal-forge-builder = {
      wantedBy = lib.mkForce [];
      path = [pkgs.openssh];
      environment.NIX_SSHOPTS = "-i /var/lib/crystal-forge/cache-ssh -o UserKnownHostsFile=/var/lib/crystal-forge/cache-known-hosts -o StrictHostKeyChecking=yes";
    };
    nix.settings = {
      substituters = lib.mkForce [];
      builders = lib.mkForce [];
      fallback = false;
    };
    environment.systemPackages = [pkgs.crystal-forge.default.cf-keygen-drv pkgs.openssh];
  };

  # Registration precedes the first real agent report. Keys exist only inside
  # this run, never in immutable fixtures or evidence receipts.
  target = {}: {...}: {
    imports = [inputs.self.nixosModules.crystal-forge];
    virtualisation.memorySize = 1024;
    services.crystal-forge = {
      enable = true;
      local-database = false;
      server.enable = false;
      build.enable = false;
      client = {
        enable = true;
        package = packages.agent;
        server_host = "controller";
        server_port = 3000;
        private_key = "/run/cf-it-agent.key";
      };
    };
    systemd.services.crystal-forge-agent.wantedBy = lib.mkForce [];
    environment.systemPackages = [pkgs.crystal-forge.default.cf-keygen-drv];
  };
}
