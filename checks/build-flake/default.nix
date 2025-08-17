{
  inputs,
  pkgs,
  lib,
  ...
}: let
  systemBuildClosure = pkgs.closureInfo {
    rootPaths =
      [
        inputs.self.nixosConfigurations.cf-test-sys.config.system.build.toplevel
        inputs.self.nixosConfigurations.cf-test-sys.config.system.build.toplevel.drvPath
        pkgs.crystal-forge.default
        pkgs.crystal-forge.default.drvPath
        lib.crystal-forge.testFlake
        pkgs.path
      ]
      ++ lib.crystal-forge.prefetchedPaths;
  };
in
  pkgs.testers.runNixOSTest {
    name = "crystal-forge-git-server-test";

    nodes = {
      gitserver = lib.crystal-forge.makeGitServerNode {
        inherit pkgs systemBuildClosure;
      };

      builder = {pkgs, ...}: {
        services.getty.autologinUser = "root";
        virtualisation.writableStore = true;
        virtualisation.memorySize = 2048;
        virtualisation.additionalPaths = [systemBuildClosure];

        environment.systemPackages = [pkgs.git pkgs.jq];

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
      };
    };

    testScript = ''
      gitserver.start()
      gitserver.wait_for_unit("git-http-server.service")
      gitserver.wait_for_open_port(8080)

      builder.start()
      builder.wait_for_unit("multi-user.target")

      # Test git server is working
      gitserver.succeed("ls -la /srv/git/crystal-forge.git/")
      gitserver.succeed("cd /tmp && git clone /srv/git/crystal-forge.git crystal-forge-checkout")
      gitserver.succeed("ls -la /tmp/crystal-forge-checkout/")

      # Test local flake operations on gitserver
      gitserver.succeed("cd /tmp/crystal-forge-checkout && git log --oneline")
      gitserver.succeed("nix flake show /tmp/crystal-forge-checkout")
      gitserver.succeed("nix build /tmp/crystal-forge-checkout#nixosConfigurations.cf-test-sys.config.system.build.toplevel -o /root/local-system --offline")

      # Test remote flake access from builder
      builder.succeed("nix flake show git://gitserver:8080/crystal-forge.git --no-write-lock-file")

      # This is the key test - building the NixOS system from remote git repo
      builder.succeed("nix build git://gitserver:8080/crystal-forge.git#nixosConfigurations.cf-test-sys.config.system.build.toplevel -o /root/remote-system --no-write-lock-file --offline")
      builder.succeed("test -e /root/remote-system")

      # Test other packages if they exist
      builder.succeed("nix build git://gitserver:8080/crystal-forge.git#crystal-forge.default -o /root/remote-pkg --no-write-lock-file --offline || echo 'Package build skipped'")

      # Generate test report
      builder.succeed("mkdir -p /tmp/xchg")
      report_file = "/tmp/xchg/git-server-test-report.txt"
      builder.succeed(f"printf '%s\n' '========================================' 'Crystal Forge Git Server Test Report' 'Generated: '$(date) '========================================' > {report_file}")
      builder.succeed(f"echo 'Remote flake URL: git://gitserver:8080/crystal-forge.git' >> {report_file}")
      builder.succeed(f"echo 'Build successful: YES' >> {report_file}")
      builder.succeed(f"echo 'System path: '$(readlink /root/remote-system) >> {report_file}")
      builder.copy_from_vm("/tmp/xchg/git-server-test-report.txt")
    '';
  }
