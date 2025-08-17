{
  inputs,
  pkgs,
  lib,
  ...
}: let
  testFlake = pkgs.crystal-forge.flake-with-git;

  lockJson = builtins.fromJSON (builtins.readFile "${testFlake}/flake.lock");
  nodes = lockJson.nodes;

  prefetchNode = name: node: let
    l = node.locked or {};
  in
    if (l.type or "") == "github"
    then {
      key = "github:${l.owner}/${l.repo}";
      path = builtins.fetchTree {
        type = "github";
        owner = l.owner;
        repo = l.repo;
        rev = l.rev;
        narHash = l.narHash;
      };
      from = {
        type = "github";
        owner = l.owner;
        repo = l.repo;
      };
    }
    else if (l.type or "") == "git"
    then {
      key = "git:${l.url}";
      path = builtins.fetchTree {
        type = "git";
        url = l.url;
        rev = l.rev;
        narHash = l.narHash;
      };
      from = {
        type = "git";
        url = l.url;
      };
    }
    else if (l.type or "") == "tarball"
    then {
      key = "tarball:${l.url}";
      path = builtins.fetchTree {
        type = "tarball";
        url = l.url;
        narHash = l.narHash;
      };
      from = {
        type = "tarball";
        url = l.url;
      };
    }
    else null;

  prefetchedList = lib.pipe nodes [
    (lib.mapAttrsToList prefetchNode)
    (builtins.filter (x: x != null))
  ];

  prefetchedPaths = map (x: x.path) prefetchedList;

  registryEntries = lib.listToAttrs (map
    (x:
      lib.nameValuePair
      (builtins.replaceStrings [":" "/" "."] ["-" "-" "-"] x.key)
      {
        from = x.from;
        to = {
          type = "path";
          path = x.path;
        };
      })
    prefetchedList);
in
  pkgs.testers.runNixOSTest {
    name = "crystal-forge-flake-with-git-test";

    nodes = {
      testNode = {pkgs, ...}: {
        services.getty.autologinUser = "root";
        virtualisation.writableStore = true;
        virtualisation.memorySize = 2048;

        # Include the full closure of the package being tested
        virtualisation.additionalPaths =
          [
            testFlake
            pkgs.path
          ]
          ++ prefetchedPaths
          ++ [
            # Add just the default crystal-forge package
            pkgs.crystal-forge.default
          ];

        nix = {
          package = pkgs.nixVersions.stable;
          settings = {
            experimental-features = ["nix-command" "flakes"];
            substituters = [];
            builders-use-substitutes = false;
            fallback = false;
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
      testNode.succeed(f"cd {work_dir} && nix flake show --offline >> {report_file} 2>&1 || echo 'offline flake show failed' >> {report_file}")
      testNode.succeed(f"echo >> {report_file}")

      testNode.succeed(f"echo 'FLAKE METADATA (offline):' >> {report_file}")
      testNode.succeed(f"echo '==========================' >> {report_file}")
      testNode.succeed(f"cd {work_dir} && nix flake metadata --offline >> {report_file} 2>&1 || echo 'offline flake metadata failed' >> {report_file}")
      testNode.succeed(f"echo >> {report_file}")

      testNode.succeed(f"echo 'BUILD TEST (offline):' >> {report_file}")
      testNode.succeed(f"echo '=====================' >> {report_file}")
      testNode.succeed(f"cd {work_dir} && nix build .#default --offline -o /tmp/flake-build >> {report_file} 2>&1 || true")
      testNode.succeed(f"echo >> {report_file}")

      testNode.succeed(f"echo 'BUILD VERIFICATION:' >> {report_file}")
      testNode.succeed(f"echo '==================' >> {report_file}")
      testNode.succeed(f"[ -e /tmp/flake-build ] && echo 'Build successful: YES  -> '$(readlink /tmp/flake-build) >> {report_file} || echo 'Build successful: NO' >> {report_file}")
      testNode.succeed(f"[ -e /tmp/flake-build ] || (echo 'Packages available:' >> {report_file}; cd {work_dir}; nix flake show --json 2>/dev/null | jq -r '..|.packages? // empty | to_entries[] | .key' >> {report_file} || true)")
      testNode.succeed(f"echo >> {report_file}")

      testNode.copy_from_vm("/tmp/xchg/flake-with-git-test-report.txt")
    '';
  }
