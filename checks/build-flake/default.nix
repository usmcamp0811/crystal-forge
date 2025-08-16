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
  toyFlakeDir = pkgs.runCommand "toyflake-dir" {} ''
    mkdir -p $out

    # Write the flake.nix that references pre-built system
    cat > $out/flake.nix << 'EOF'
    {
      inputs = {};
      outputs = { self }:
      let
        system = "${system}";
        pkgs = import <nixpkgs> { inherit system; };
      in {
        packages.''${system}.hello = ${helloPackage};
        defaultPackage.''${system} = self.packages.''${system}.hello;

        nixosConfigurations.cf-test-sys = {
          config.system.build.toplevel = ${nixosSystemToplevel};
        };
      };
    }
    EOF

    # Create an empty flake.lock
    cat > $out/flake.lock << 'EOF'
    {
      "nodes": {
        "root": {
          "inputs": {},
          "locked": {
            "lastModified": 1,
            "narHash": "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "type": "file"
          }
        }
      },
      "root": "root",
      "version": 7
    }
    EOF
  '';
in
  pkgs.testers.runNixOSTest {
    name = "build-nixos-flake-offline-in-vm";
    nodes.builder = {pkgs, ...}: {
      services.getty.autologinUser = "root";
      virtualisation.writableStore = true;
      virtualisation.memorySize = 2048; # More memory for system builds

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

    testScript = ''
      builder.start()
      builder.wait_for_unit("multi-user.target")

      # Test package build
      builder.succeed("nix build /etc/toyflake#hello -o /root/pkg --impure")
      builder.succeed("/root/pkg/bin/hello | grep hello-from-flake")

      # Test nixos system build
      builder.succeed("nix build /etc/toyflake#nixosConfigurations.cf-test-sys.config.system.build.toplevel -o /root/system --impure")
      builder.succeed("test -e /root/system")
      builder.succeed("test -x /root/system/bin/switch-to-configuration")
      builder.succeed("test -s /root/system/nixos-version")

      # Optional: verify it's a proper nixos system
      builder.succeed("ls -la /root/system/")
    '';
  }
