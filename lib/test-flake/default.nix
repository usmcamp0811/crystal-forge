{
  lib,
  inputs,
  system ? null,
  ...
}: let
  inherit (lockJson) nodes;
  # Default to x86_64-linux if no system specified
  actualSystem =
    if system != null
    then system
    else "x86_64-linux";

  pkgs = inputs.nixpkgs.legacyPackages.${actualSystem};

  srcPath = ./test-flake;

  # Parse the flake.lock file to get dependency information
  lockJson = builtins.fromJSON (builtins.readFile (srcPath + "/flake.lock"));
  # Process each dependency node from flake.lock and prefetch it
  # Returns null for unsupported dependency types
  prefetchNode = name: node: let
    l = node.locked or {};
  in
    # Handle GitHub repositories
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
    # Handle Git repositories
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
    # Handle tarball sources
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
    # Return null for unsupported types (filtered out later)
    else null;

  # Process all nodes from flake.lock and filter out unsupported ones
  # Results in a list of { key, path, from } records for each dependency
  prefetchedList = lib.pipe nodes [
    (lib.mapAttrsToList prefetchNode)
    (builtins.filter (x: x != null))
  ];

  # Extract just the Nix store paths for use in virtualisation.additionalPaths
  prefetchedPaths = map (x: x.path) prefetchedList;

  # Create registry entries that map flake references to local paths
  # This allows offline flake resolution by redirecting to prefetched dependencies
  # Registry keys have special characters replaced to be valid attribute names
  registryEntries = lib.listToAttrs (map
    (x:
      lib.nameValuePair
      # Sanitize the key for use as an attribute name
      (builtins.replaceStrings [":" "/" "."] ["-" "-" "-"] x.key)
      {
        # Original flake reference
        from = x.from;
        # Redirect to local path
        to = {
          type = "path";
          path = x.path;
        };
      })
    prefetchedList);

  # Create a bare git repository directly from the flake source for serving
  # This simulates a real git-tracked flake environment with development history
  testFlake =
    pkgs.runCommand "crystal-forge-test-flake.git" {
      nativeBuildInputs = [pkgs.git];
    } ''
      set -eu
      export HOME=$PWD
      work="$TMPDIR/work"
      # Prepare working directory
      mkdir -p "$work"
      cp -r ${srcPath}/. "$work/"
      cd "$work"
      chmod -R u+w .
      git init -q
      git config user.name "Crystal Forge Test"
      git config user.email "test@crystal-forge.dev"

      # Array to store commit hashes for each branch
      declare -A BRANCH_COMMITS
      BRANCH_COMMITS[main]=""
      BRANCH_COMMITS[development]=""
      BRANCH_COMMITS[feature/experimental]=""

      # === MAIN BRANCH (5 commits) ===
      git checkout -b main

      git add -f flake.nix
      [ -f flake.lock ] && git add -f flake.lock
      git commit -q -m "Initial flake configuration"
      BRANCH_COMMITS[main]+="$(git rev-parse HEAD) "

      echo "# Crystal Forge Production Environment" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Add production documentation"
      BRANCH_COMMITS[main]+="$(git rev-parse HEAD) "

      echo "# Project files added" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Add project files for production"
      BRANCH_COMMITS[main]+="$(git rev-parse HEAD) "

      echo "# Version: 1.0.0-stable" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Release version 1.0.0"
      BRANCH_COMMITS[main]+="$(git rev-parse HEAD) "

      echo "# Production ready" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Mark as production ready"
      BRANCH_COMMITS[main]+="$(git rev-parse HEAD) "

      MAIN_HEAD=$(git rev-parse HEAD)

      # === DEVELOPMENT BRANCH (7 commits) ===
      git checkout -b development main

      echo "# Development Environment" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Setup development environment"
      BRANCH_COMMITS[development]+="$(git rev-parse HEAD) "

      echo "# Debug flags enabled" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Enable debug flags for development"
      BRANCH_COMMITS[development]+="$(git rev-parse HEAD) "

      echo "# Development dependencies" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Add development dependencies"
      BRANCH_COMMITS[development]+="$(git rev-parse HEAD) "

      echo "# Testing framework integration" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Integrate testing framework"
      BRANCH_COMMITS[development]+="$(git rev-parse HEAD) "

      echo "# Hot reload support" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Add hot reload for development"
      BRANCH_COMMITS[development]+="$(git rev-parse HEAD) "

      echo "# Development tools configured" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Configure development tools"
      BRANCH_COMMITS[development]+="$(git rev-parse HEAD) "

      echo "# Latest development snapshot" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Development snapshot v1.1.0-dev"
      BRANCH_COMMITS[development]+="$(git rev-parse HEAD) "

      DEV_HEAD=$(git rev-parse HEAD)

      # === FEATURE BRANCH (3 commits) ===
      git checkout -b feature/experimental development

      echo "# Experimental features enabled" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Enable experimental features"
      BRANCH_COMMITS[feature/experimental]+="$(git rev-parse HEAD) "

      echo "# New algorithm implementation" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Implement new experimental algorithm"
      BRANCH_COMMITS[feature/experimental]+="$(git rev-parse HEAD) "

      echo "# Performance benchmarks" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Add performance benchmarks"
      BRANCH_COMMITS[feature/experimental]+="$(git rev-parse HEAD) "

      FEATURE_HEAD=$(git rev-parse HEAD)

      # Create bare repository for serving
      git init --bare "$out"
      git -C "$out" config receive.denyCurrentBranch ignore

      # Push all branches to bare repo
      git push "$out" main:refs/heads/main
      git push "$out" development:refs/heads/development
      git push "$out" feature/experimental:refs/heads/feature/experimental

      # Set main as default branch
      git -C "$out" symbolic-ref HEAD refs/heads/main

      # Enable Git HTTP backend
      git -C "$out" config http.receivepack true
      git -C "$out" config http.uploadpack true
      git -C "$out" update-server-info

      # Output branch information
      echo "$MAIN_HEAD" > "$out/MAIN_HEAD"
      echo "$DEV_HEAD" > "$out/DEVELOPMENT_HEAD"
      echo "$FEATURE_HEAD" > "$out/FEATURE_HEAD"

      # Output all commits for each branch (space-separated)
      echo "''${BRANCH_COMMITS[main]}" | tr ' ' '\n' | grep -v '^$' > "$out/MAIN_COMMITS"
      echo "''${BRANCH_COMMITS[development]}" | tr ' ' '\n' | grep -v '^$' > "$out/DEVELOPMENT_COMMITS"
      echo "''${BRANCH_COMMITS[feature/experimental]}" | tr ' ' '\n' | grep -v '^$' > "$out/FEATURE_COMMITS"

      # Output commit counts for each branch
      echo "5" > "$out/MAIN_COMMIT_COUNT"
      echo "7" > "$out/DEVELOPMENT_COMMIT_COUNT"
      echo "3" > "$out/FEATURE_COMMIT_COUNT"

      # Legacy compatibility - use main branch data
      echo "$MAIN_HEAD" > "$out/HEAD_COMMIT"
      echo "''${BRANCH_COMMITS[main]}" | tr ' ' '\n' | grep -v '^$' > "$out/ALL_COMMITS"
      echo "5" > "$out/COMMIT_COUNT"
    '';

  # Function to create derivation-paths.json
  derivation-paths = pkgs:
    pkgs.runCommand "derivation-paths.json" {
      nativeBuildInputs = [pkgs.nix pkgs.git];
      testFlakeSource = testFlake;
      NIX_CONFIG = "experimental-features = nix-command flakes";
      NIXPKGS_PATH = pkgs.path;
    } ''
            set -euo pipefail

            export HOME=$TMPDIR
            export NIX_USER_PROFILE_DIR=$TMPDIR/profiles
            export NIX_PROFILES="$NIX_USER_PROFILE_DIR/profile"
            mkdir -p "$NIX_USER_PROFILE_DIR"

            flake="git+file://$testFlakeSource?ref=main"

            cf_test_sys_drv=$(
              nix eval --impure --raw \
                --override-input nixpkgs "path:$NIXPKGS_PATH" \
                "$flake#nixosConfigurations.cf-test-sys.config.system.build.toplevel.drvPath"
            )

            test_agent_drv=$(
              nix eval --impure --raw \
                --override-input nixpkgs "path:$NIXPKGS_PATH" \
                "$flake#nixosConfigurations.test-agent.config.system.build.toplevel.drvPath"
            )

            cat >"$out" <<EOF
      {
        "cf-test-sys": {
          "derivation_path": "$cf_test_sys_drv",
          "derivation_name": "cf-test-sys",
          "derivation_type": "nixos"
        },
        "test-agent": {
          "derivation_path": "$test_agent_drv",
          "derivation_name": "test-agent",
          "derivation_type": "nixos"
        }
      }
      EOF
    '';

  # Function to create a test VM with testFlake at a specific commit
  # Usage: mkTestVm { commit = "abc123"; branch = "main"; vmConfig = { ... }; withGitServer = true; }
  mkTestVm = {
    commit ? null,
    branch ? "main",
    vmConfig ? {},
    withGitServer ? false,
    gitServerPort ? 8080,
    ...
  }: let
    # Create flake reference with commit or branch
    flakeRef =
      if commit != null
      then "git+file://${testFlake}?rev=${commit}"
      else "git+file://${testFlake}?ref=${branch}";

    # Git URL for cloning when using git server
    gitUrl = "git://gitserver:${toString gitServerPort}/crystal-forge.git";

    systemBuildClosure = pkgs.closureInfo {
      rootPaths = [pkgs.path] ++ prefetchedPaths;
    };
  in
    pkgs.nixosTest {
      name = "crystal-forge-test-vm-${
        if commit != null
        then commit
        else branch
      }";

      nodes =
        {
          testvm = {
            config,
            pkgs,
            ...
          }:
            lib.mkMerge [
              {
                # Base VM configuration
                virtualisation = {
                  memorySize = 2048;
                  cores = 2;
                  graphics = false;
                  additionalPaths = prefetchedPaths;
                  diskSize = 8192;
                  writableStore = withGitServer; # Allow rebuilds when using git server
                };

                # Add nix registry entries for offline resolution
                nix.registry = registryEntries;

                # Enable flakes and make testFlake available
                nix.settings.experimental-features = ["nix-command" "flakes"];

                # Pre-configure git for flake operations
                programs.git = {
                  enable = true;
                  config = {
                    user.name = "Test User";
                    user.email = "test@crystal-forge.dev";
                  };
                };

                # Make testFlake available as environment variable
                environment.variables =
                  {
                    CRYSTAL_FORGE_TEST_FLAKE = flakeRef;
                  }
                  // lib.optionalAttrs withGitServer {
                    CRYSTAL_FORGE_GIT_URL = gitUrl;
                  };

                # Add helper scripts
                environment.systemPackages =
                  [
                    (pkgs.writeScriptBin "cf-flake" ''
                      #!${pkgs.bash}/bin/bash
                      exec nix --experimental-features "nix-command flakes" "$@" "$CRYSTAL_FORGE_TEST_FLAKE"
                    '')
                  ]
                  ++ lib.optionals withGitServer [
                    (pkgs.writeScriptBin "cf-clone-and-switch" ''
                      #!${pkgs.bash}/bin/bash
                      set -euo pipefail

                      # Clone the repository
                      if [ ! -d /tmp/crystal-forge ]; then
                        echo "Cloning from $CRYSTAL_FORGE_GIT_URL..."
                        git clone "$CRYSTAL_FORGE_GIT_URL" /tmp/crystal-forge
                      fi

                      cd /tmp/crystal-forge

                      # Checkout specific commit or branch
                      ${
                        if commit != null
                        then ''
                          echo "Checking out commit ${commit}..."
                          git checkout ${commit}
                        ''
                        else ''
                          echo "Checking out branch ${branch}..."
                          git checkout ${branch}
                        ''
                      }

                      # Show current state
                      echo "Current commit: $(git rev-parse HEAD)"
                      echo "Available configurations:"
                      nix flake show

                      # Switch to configuration if specified
                      if [ $# -gt 0 ]; then
                        echo "Switching to configuration: $1"
                        nixos-rebuild switch --flake ".#$1"
                      else
                        echo "Usage: cf-clone-and-switch <configuration-name>"
                        echo "Available configurations:"
                        nix eval --json .#nixosConfigurations --apply builtins.attrNames
                      fi
                    '')
                  ];
              }
              vmConfig
            ];
        }
        // lib.optionalAttrs withGitServer {
          # Add git server node when requested
          gitserver = lib.crystal-forge.makeGitServerNode {
            inherit pkgs systemBuildClosure;
            port = gitServerPort;
          };
        };

      testScript = ''
        start_all()

        ${lib.optionalString withGitServer ''
          # Wait for git server
          gitserver.wait_for_unit("multi-user.target")
          gitserver.wait_for_unit("git-daemon.service")
          gitserver.wait_for_open_port(${toString gitServerPort})
        ''}

        testvm.wait_for_unit("multi-user.target")

        # Verify testFlake is accessible
        testvm.succeed("cf-flake show --json")

        # Verify we can evaluate the flake
        testvm.succeed("nix eval $CRYSTAL_FORGE_TEST_FLAKE#description || echo 'No description found'")

        ${lib.optionalString withGitServer ''
          # Test git server connectivity
          testvm.succeed("ping -c 1 gitserver")

          # Test cloning and show available configurations
          testvm.succeed("cf-clone-and-switch")
        ''}
      '';
    };

  # Function to preload testFlake at a specific commit into any VM configuration
  # Usage: preloadTestFlake { commit = "abc123"; branch = "main"; path = "/opt/crystal-forge"; }
  preloadTestFlake = {
    commit ? null,
    branch ? "main",
    path ? "/opt/crystal-forge",
    ...
  }: let
    # Create a checkout of testFlake at the specified commit/branch
    flakeCheckout =
      pkgs.runCommand "preloaded-test-flake" {
        nativeBuildInputs = [pkgs.git];
      } ''
        set -euo pipefail
        export HOME=$TMPDIR

        # Clone the bare repo
        git clone ${testFlake} $out
        cd $out

        # Checkout specific commit or branch
        ${
          if commit != null
          then ''
            git checkout ${commit}
          ''
          else ''
            git checkout ${branch}
          ''
        }

        # Remove .git to make it a clean source tree
        rm -rf .git

        # Add a marker file with checkout info
        cat > PRELOADED_INFO <<EOF
        Preloaded testFlake
        ${
          if commit != null
          then "Commit: ${commit}"
          else "Branch: ${branch}"
        }
        Checked out at: $(date)
        EOF
      '';
  in {
    # Configuration to add to any VM
    environment.etc."preloaded-flake".source = flakeCheckout;

    # Helper script to use the preloaded flake
    environment.systemPackages = [
      (pkgs.writeScriptBin "use-preloaded-flake" ''
        #!${pkgs.bash}/bin/bash
        set -euo pipefail

        target_path="${path}"

        # Copy preloaded flake to target location
        if [ ! -d "$target_path" ]; then
          echo "Copying preloaded flake to $target_path..."
          mkdir -p "$(dirname "$target_path")"
          cp -r /etc/preloaded-flake "$target_path"
          chmod -R u+w "$target_path"
        fi

        cd "$target_path"

        # Show preload info
        cat PRELOADED_INFO
        echo ""

        # Show available configurations
        echo "Available NixOS configurations:"
        nix flake show --json | ${pkgs.jq}/bin/jq -r '.nixosConfigurations | keys[]'
        echo ""

        # Execute command if provided
        if [ $# -gt 0 ]; then
          exec "$@"
        else
          echo "Usage: use-preloaded-flake [command...]"
          echo "Examples:"
          echo "  use-preloaded-flake nix flake show"
          echo "  use-preloaded-flake nixos-rebuild switch --flake .#cf-test-sys"
        fi
      '')
    ];

    # Environment variable pointing to the preloaded flake
    environment.variables = {
      PRELOADED_CRYSTAL_FORGE_FLAKE = path;
    };
  };
in {
  inherit
    lockJson # Parsed flake.lock content
    nodes # Dependency nodes from flake.lock
    prefetchedList # List of { key, path, from } records
    prefetchedPaths # List of Nix store paths for dependencies
    registryEntries # Registry mappings for offline resolution
    testFlake # Bare git repository for testing
    derivation-paths # Function to generate derivation paths
    mkTestVm # Function to create test VMs at specific commits
    preloadTestFlake # Function to preload testFlake into any VM
    ;
}
