{
  lib,
  inputs,
  system ? null,
  ...
}: rec {
  # Create a reusable git server node for tests with cgit web interface
  # This provides a standardized git server that can serve repositories over git protocol and HTTP
  makeGitServerNode = {
    port ? 8080,
    extraConfig ? {},
    systemBuildClosure,
    pkgs,
  }:
    {
      services.getty.autologinUser = "root";
      networking.firewall.allowedTCPPorts = [port 80];
      virtualisation.writableStore = true;
      virtualisation.memorySize = 2048;
      virtualisation.additionalPaths = [systemBuildClosure];

      environment.systemPackages = [pkgs.git pkgs.jq pkgs.crystal-forge.cf-test-modules.runTests pkgs.crystal-forge.cf-test-modules.testRunner];

      nix = {
        package = pkgs.nixVersions.stable;
        settings = {
          experimental-features = ["nix-command" "flakes"];
          substituters = [];
          builders-use-substitutes = true;
          fallback = true;
          sandbox = true;
          keep-outputs = true;
          keep-derivations = true;
        };
        extraOptions = ''
          accept-flake-config = true
          flake-registry = ${pkgs.writeText "empty-registry.json" ''{"flakes":[]}''}
        '';
        registry =
          lib.crystal-forge.registryEntries
          // {
            nixpkgs = {
              to = {
                type = "path";
                path = pkgs.path;
              };
            };
          };
      };

      nix.nixPath = ["nixpkgs=${pkgs.path}"];

      # Create git user for proper permissions
      users.users.git = {
        isSystemUser = true;
        group = "git";
        home = "/srv/git";
        createHome = true;
      };
      users.groups.git = {};

      # Create cgit user
      users.users.cgit = {
        isSystemUser = true;
        group = "cgit";
        home = "/var/lib/cgit";
      };
      users.groups.cgit = {};

      systemd.tmpfiles.rules = [
        "d /srv/git 0755 git git -"
        "d /var/lib/cgit 0755 cgit cgit -"
      ];

      # Service to create writable git repository
      # makeGitServerNode: setup bare repo safely from a Nix store source
      systemd.services.setup-git-repo = {
        enable = true;
        description = "Initialize bare test git repository";
        after = ["systemd-tmpfiles-setup.service"];
        before = ["git-daemon.service"];
        wantedBy = ["multi-user.target"];

        serviceConfig = {
          Type = "oneshot";
          User = "git";
          Group = "git";
          ExecStart = pkgs.writeShellScript "init-bare-crystal-forge-repo" ''
            set -euo pipefail
            src='${lib.crystal-forge.testFlake}'
            dst='/srv/git/crystal-forge.git'

            mkdir -p /srv/git
            # Only (re)create if missing
            if [ ! -d "$dst" ]; then
              # Allow cloning from a repo in the Nix store (.git owned by root)
              ${pkgs.git}/bin/git \
                -c safe.directory='*' \
                clone --bare "$src" "$dst"

              ${pkgs.git}/bin/git -C "$dst" update-server-info
              chown -R git:git "$dst"
              chmod -R u+rwX,go+rX "$dst"
            fi
          '';
          RemainAfterExit = true;
        };
      };

      environment.etc."gitconfig".text = ''
        [safe]
            directory = /srv/git/crystal-forge.git
      '';

      # Configure cgit web interface with better service dependencies
      services.cgit = {
        gitserver = {
          enable = true;
          scanPath = "/srv/git";

          settings = {
            root-title = "Crystal Forge Git Server";
            root-desc = "Test Git repositories for Crystal Forge";
            enable-follow-links = true;
            enable-index-links = true;
            enable-log-filecount = true;
            enable-log-linecount = true;
            source-filter = "${pkgs.cgit}/lib/cgit/filters/syntax-highlighting.py";
            about-filter = "${pkgs.cgit}/lib/cgit/filters/about-formatting.sh";
            cache-size = 1000;
            enable-git-config = true;
          };

          nginx = {
            virtualHost = "localhost";
            location = "/";
          };

          user = "cgit";
          group = "cgit";
        };
      };

      # Override the fcgiwrap service to wait for git repo setup
      systemd.services.fcgiwrap-cgit-gitserver = {
        after = ["setup-git-repo.service"];
        wants = ["setup-git-repo.service"];

        # Add restart on failure with delay
        serviceConfig = {
          Restart = "on-failure";
          RestartSec = "5s";
          StartLimitBurst = 3;
          StartLimitIntervalSec = "30s";
        };
      };

      # Enable nginx for cgit
      services.nginx.enable = true;

      # Git daemon for git:// protocol access
      systemd.services.git-daemon = {
        enable = true;
        description = "Git Daemon for git:// protocol";
        after = ["network.target" "setup-git-repo.service"];
        wants = ["setup-git-repo.service"];
        wantedBy = ["multi-user.target"];

        serviceConfig = {
          Type = "exec";
          User = "git";
          Group = "git";
          WorkingDirectory = "/srv/git";
          ExecStart = "${pkgs.git}/bin/git daemon --verbose --export-all --base-path=/srv/git --reuseaddr --port=${toString port}";
          Environment = "HOME=/srv/git";
          Restart = "on-failure";
          RestartSec = "5s";
        };
      };
    }
    // extraConfig;
}
