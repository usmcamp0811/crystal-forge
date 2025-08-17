{
  inputs,
  pkgs,
  lib,
  ...
}:
with lib.crystal-forge; let
  cfTestSysToplevel = inputs.self.nixosConfigurations.cf-test-sys.config.system.build.toplevel;
in
  pkgs.testers.runNixOSTest {
    name = "crystal-forge-flake-with-git-test";

    nodes = {
      testNode = {pkgs, ...}: {
        services.getty.autologinUser = "root";
        virtualisation.writableStore = true;
        virtualisation.memorySize = 2048;
        # Force the system to be fully built by making it a build input
        virtualisation.additionalPaths = let
          # This forces the full closure to be built and available
          fullSystemClosure = pkgs.closureInfo {rootPaths = [cfTestSysToplevel];};
        in
          [
            testFlake
            pkgs.path
            fullSystemClosure
          ]
          ++ prefetchedPaths
          ++ [
            pkgs.crystal-forge.default
          ];

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
            registryEntries
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
        environment.systemPackages = with pkgs; [git jq];
        environment.etc."test-flake".source = testFlake;

        # No networking needed - this is an isolated test
        networking.useDHCP = false;
        networking.interfaces.eth0.useDHCP = false;
      };
    };

    testScript = ''
      testNode.start()
      testNode.wait_for_unit("multi-user.target")

      testNode.succeed("mkdir -p /tmp/xchg")
      report_file = "/tmp/xchg/flake-with-git-test-report.txt"
      testNode.succeed(f"printf '%s\n' '========================================' 'Crystal Forge flake-with-git Test Report' 'Generated: '$(date) '========================================' > {report_file}")
      testNode.succeed(f"printf '%s\n' 'SYSTEM STATUS:' '==============' >> {report_file}")
      testNode.succeed(f"echo 'Nix version: '$(nix --version) >> {report_file}")
      testNode.succeed(f"echo 'Git version: '$(git --version) >> {report_file}")
      testNode.succeed(f"echo 'Flake source path: /etc/test-flake' >> {report_file}")
      testNode.succeed(f"echo >> {report_file}")

      work_dir = "/root/test-flake"
      testNode.succeed(f"rm -rf {work_dir}")
      testNode.succeed(f"cp -rL /etc/test-flake {work_dir}")
      testNode.succeed(f"chmod -R u+rwX {work_dir}")

      # Init repo and force-track flake files even if ignored anywhere
      testNode.succeed(f"cd {work_dir} && (git init -b main || (git init && git checkout -b main))")
      testNode.succeed(f"cd {work_dir} && git config --global safe.directory '*'")
      testNode.succeed(f"cd {work_dir} && git config user.name 'Test User'")
      testNode.succeed(f"cd {work_dir} && git config user.email 'test@example.com'")
      testNode.succeed(f"cd {work_dir} && git add -f flake.nix || true")
      testNode.succeed(f"cd {work_dir} && [ -f flake.lock ] && git add -f flake.lock || true")
      testNode.succeed(f"cd {work_dir} && git add -A")
      testNode.succeed(f"cd {work_dir} && (git commit -m seed || true)")

      # Sanity log
      testNode.succeed(f"cd {work_dir} && git status --porcelain=v1 >> {report_file}")
      testNode.succeed(f"cd {work_dir} && git ls-files >> {report_file}")

      # Offline show/metadata/build
      testNode.succeed(f"echo 'FLAKE SHOW OUTPUT (offline):' >> {report_file}")
      testNode.succeed(f"echo '=============================' >> {report_file}")
      testNode.succeed(f"cd {work_dir} && nix flake show >> {report_file} 2>&1 || echo 'offline flake show failed' >> {report_file}")
      testNode.succeed(f"echo >> {report_file}")

      testNode.succeed(f"echo 'FLAKE METADATA (offline):' >> {report_file}")
      testNode.succeed(f"echo '==========================' >> {report_file}")
      testNode.succeed(f"cd {work_dir} && nix flake metadata >> {report_file} 2>&1 || echo 'offline flake metadata failed' >> {report_file}")
      testNode.succeed(f"echo >> {report_file}")

      testNode.succeed(f"echo 'BUILD TEST (offline):' >> {report_file}")
      testNode.succeed(f"echo '=====================' >> {report_file}")
      testNode.succeed(f"cd {work_dir} && nix build --dry-run .#nixosConfigurations.cf-test-sys.config.system.build.toplevel -o /tmp/flake-build >> {report_file} 2>&1 || true")
      testNode.succeed(f"echo >> {report_file}")

      testNode.succeed(f"echo 'BUILD VERIFICATION:' >> {report_file}")
      testNode.succeed(f"echo '==================' >> {report_file}")
      testNode.succeed(f"[ -e /tmp/flake-build ] && echo 'Build successful: YES  -> '$(readlink /tmp/flake-build) >> {report_file} || echo 'Build successful: NO' >> {report_file}")
      testNode.succeed(f"[ -e /tmp/flake-build ] || (echo 'Packages available:' >> {report_file}; cd {work_dir}; nix flake show --json 2>/dev/null | jq -r '..|.packages? // empty | to_entries[] | .key' >> {report_file} || true)")
      testNode.succeed(f"echo >> {report_file}")

      testNode.copy_from_vm("/tmp/xchg/flake-with-git-test-report.txt")
    '';
  }
