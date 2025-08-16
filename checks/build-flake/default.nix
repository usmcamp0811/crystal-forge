{
  pkgs,
  system,
  ...
}: let
  lib = pkgs.lib;

  # Pre-build a hello package
  helloPackage = pkgs.writeShellApplication {
    name = "hello";
    text = "echo hello-from-flake\n";
  };

  # Pre-build a minimal NixOS system on the host
  nixosSystemToplevel =
    (import (pkgs.path + "/nixos/lib/eval-config.nix") {
      inherit system;
      modules = [
        {
          # Minimal configuration
          boot.isContainer = true;
          fileSystems."/" = {
            device = "none";
            fsType = "tmpfs";
          };
          services.getty.autologinUser = "root";
          environment.systemPackages = [helloPackage];
          system.stateVersion = "25.05";

          # Disable unnecessary services for faster build
          services.udisks2.enable = false;
          security.polkit.enable = false;
          documentation.enable = false;
          documentation.nixos.enable = false;

          # Disable NSS modules instead of nscd to avoid the assertion error
          system.nssModules = lib.mkForce [];
        }
      ];
    }).config.system.build.toplevel;

  # Create a flake that references the pre-built system
  # Create a flake that references the pre-built system
  toyFlakeDir = pkgs.runCommand "toyflake-dir" {} ''
    mkdir -p $out

    # Write the flake.nix that references pre-built system
    cat > $out/flake.nix << 'EOF'
    {
      inputs = {
        nixpkgs.url = "path:${pkgs.path}";
      };
      outputs = { self, nixpkgs }:
      let
        system = "${system}";
        pkgs = nixpkgs.legacyPackages.${system};
      in {
        packages.${system}.hello = pkgs.writeShellApplication {
          name = "hello";
          text = "echo hello-from-flake\n";
        };

        packages.${system}.default = self.packages.${system}.hello;

        nixosConfigurations.cf-test-sys = nixpkgs.lib.nixosSystem {
          inherit system;
          modules = [{
            boot.isContainer = true;
            fileSystems."/" = {
              device = "none";
              fsType = "tmpfs";
            };
            services.getty.autologinUser = "root";
            environment.systemPackages = [ self.packages.${system}.hello ];
            system.stateVersion = "25.05";
            services.udisks2.enable = false;
            security.polkit.enable = false;
            documentation.enable = false;
            documentation.nixos.enable = false;
            system.nssModules = pkgs.lib.mkForce [];
          }];
        };
      };
    }
    EOF

    # Create a flake.lock with nixpkgs
    cat > $out/flake.lock << 'EOF'
    {
      "nodes": {
        "nixpkgs": {
          "locked": {
            "lastModified": 1,
            "narHash": "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "path": "${pkgs.path}",
            "type": "path"
          },
          "original": {
            "path": "${pkgs.path}",
            "type": "path"
          }
        },
        "root": {
          "inputs": {
            "nixpkgs": "nixpkgs"
          }
        }
      },
      "root": "root",
      "version": 7
    }
    EOF
  '';

  # Create a bare git repo containing the test flake
  testFlakeGit = pkgs.runCommand "crystal-forge-test-flake.git" {buildInputs = [pkgs.git];} ''
    set -eu
    export HOME=$PWD
    work="$TMPDIR/w"
    mkdir -p "$work"
    cp -r ${toyFlakeDir}/* "$work"/
    chmod -R u+rwX "$work"

    git -C "$work" init
    git -C "$work" config user.name "Crystal Forge Test"
    git -C "$work" config user.email "test@crystal-forge.dev"
    git -C "$work" add .
    GIT_AUTHOR_DATE="1970-01-01T00:00:00Z" \
    GIT_COMMITTER_DATE="1970-01-01T00:00:00Z" \
      git -C "$work" commit -m "Initial test flake commit" --no-gpg-sign

    git init --bare "$out"
    git -C "$out" config receive.denyCurrentBranch ignore
    git -C "$work" push "$out" HEAD:refs/heads/main
    git -C "$out" symbolic-ref HEAD refs/heads/main

    # Enable Git HTTP backend
    git -C "$out" config http.receivepack true
    git -C "$out" config http.uploadpack true
    git -C "$out" update-server-info
  '';

  # Create a simple CGI wrapper script that bypasses fcgiwrap
  gitHttpBackendWrapper = pkgs.writeShellScript "git-http-backend-wrapper" ''
    #!/bin/sh
    export GIT_PROJECT_ROOT=/srv/git
    export GIT_HTTP_EXPORT_ALL=1
    exec ${pkgs.git}/libexec/git-core/git-http-backend "$@"
  '';
in
  pkgs.testers.runNixOSTest {
    name = "build-nixos-flake-offline-in-vm";
    nodes = {
      gitserver = {pkgs, ...}: {
        services.getty.autologinUser = "root";
        networking.firewall.allowedTCPPorts = [8080];

        environment.systemPackages = [pkgs.git pkgs.python3];

        # Create git user for proper permissions
        users.users.git = {
          isSystemUser = true;
          group = "git";
          home = "/srv/git";
          createHome = true;
        };
        users.groups.git = {};

        # Copy the test flake git repo to the expected location
        systemd.tmpfiles.rules = [
          "d /srv/git 0755 git git -"
          "L+ /srv/git/test-flake.git - - - - ${testFlakeGit}"
        ];

        # Configure git to trust the repository directory
        environment.etc."gitconfig".text = ''
          [safe]
              directory = /srv/git/test-flake.git
        '';

        # Use a simple HTTP server that can serve git repositories
        systemd.services.git-http-server = {
          enable = true;
          description = "Simple Git HTTP Server";
          after = ["network.target"];
          wantedBy = ["multi-user.target"];

          serviceConfig = {
            Type = "exec";
            User = "git";
            Group = "git";
            WorkingDirectory = "/srv/git";
            ExecStart = "${pkgs.git}/bin/git daemon --verbose --export-all --base-path=/srv/git --reuseaddr --port=8080";
            Environment = "HOME=/srv/git";
          };
        };

        # Ensure proper ownership after the symlink is created
        systemd.services.fix-git-ownership = {
          enable = true;
          description = "Fix Git Repository Ownership";
          after = ["systemd-tmpfiles-setup.service"];
          before = ["git-http-server.service"];
          wantedBy = ["multi-user.target"];

          serviceConfig = {
            Type = "oneshot";
            ExecStart = "${pkgs.bash}/bin/bash -c 'chown -R git:git /srv/git/test-flake.git'";
          };
        };
      };

      builder = {pkgs, ...}: {
        services.getty.autologinUser = "root";
        virtualisation.writableStore = true;
        virtualisation.memorySize = 2048;

        environment.systemPackages = [pkgs.git pkgs.python3];
        nix = {
          package = pkgs.nixVersions.stable;
          settings = {
            experimental-features = ["nix-command" "flakes"];
            substituters = [];
            builders-use-substitutes = false;
            fallback = false;
            sandbox = true;
          };
          extraOptions = ''
            accept-flake-config = true
            flake-registry = ${pkgs.writeText "empty-registry.json" ''{"flakes":[]}''}
          '';
        };

        # Make nixpkgs available in NIX_PATH for <nixpkgs> imports
        nix.nixPath = ["nixpkgs=${pkgs.path}"];

        # Ensure all needed closures are present in the VM image
        environment.etc = {
          toyflake.source = toyFlakeDir;
          "prefetch-hello".source = helloPackage;
          "prefetch-nixos-system".source = nixosSystemToplevel;
        };
      };
    };

    testScript = ''
      gitserver.start()
      gitserver.wait_for_unit("git-http-server.service")
      gitserver.wait_for_open_port(8080)

      builder.start()
      builder.wait_for_unit("multi-user.target")

      # Test local package build
      builder.succeed("nix build /etc/toyflake#hello -o /root/pkg --impure")
      builder.succeed("/root/pkg/bin/hello | grep hello-from-flake")

      # Test local nixos system build
      builder.succeed("nix build /etc/toyflake#nixosConfigurations.cf-test-sys.config.system.build.toplevel -o /root/system --impure")
      builder.succeed("test -e /root/system")

      # Test basic connectivity first
      gitserver.succeed("ls -la /srv/git/test-flake.git/")

      # Test remote flake access via git protocol
      builder.succeed("nix flake show git://gitserver:8080/test-flake.git --no-write-lock-file")
      builder.succeed("nix build git://gitserver:8080/test-flake.git#hello -o /root/remote-pkg --no-write-lock-file")
      builder.succeed("/root/remote-pkg/bin/hello | grep hello-from-flake")
    ''; # Replace the entire gitserver configuration with this minimal approach:
  }
