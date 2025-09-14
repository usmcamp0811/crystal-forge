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

  srcPath = ./test-flake/.;

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

  # Create a git repository directly from the flake source for serving
  # This simulates a real git-tracked flake environment with development history
  testFlake =
    pkgs.runCommand "crystal-forge-test-flake" {
      nativeBuildInputs = [pkgs.git];
    } ''
      set -eu
      export HOME=$TMPDIR

      # Configure git with safe settings for Nix sandbox
      git config --global init.defaultBranch main
      git config --global user.name "Crystal Forge Test"
      git config --global user.email "test@crystal-forge.dev"
      git config --global safe.directory "*"

      # Create the output directory and initialize git repo
      mkdir -p "$out"
      cp -r ${srcPath}/. "$out/"
      cd "$out"
      chmod -R u+w .

      git init -q

      # Array to store commit hashes for each branch
      declare -A BRANCH_COMMITS
      BRANCH_COMMITS[main]=""
      BRANCH_COMMITS[development]=""
      BRANCH_COMMITS[feature/experimental]=""

      # === MAIN BRANCH (5 commits) ===
      git checkout -b main

      git add .
      git commit -q -m "Initial flake configuration"
      BRANCH_COMMITS[main]+="$(git rev-parse HEAD) "

      echo "# Crystal Forge Production Environment" >> flake.nix
      git add flake.nix
      git commit -q -m "Add production documentation"
      BRANCH_COMMITS[main]+="$(git rev-parse HEAD) "

      echo "# Project files added" >> flake.nix
      git add flake.nix
      git commit -q -m "Add project files for production"
      BRANCH_COMMITS[main]+="$(git rev-parse HEAD) "

      echo "# Version: 1.0.0-stable" >> flake.nix
      git add flake.nix
      git commit -q -m "Release version 1.0.0"
      BRANCH_COMMITS[main]+="$(git rev-parse HEAD) "

      echo "# Production ready" >> flake.nix
      git add flake.nix
      git commit -q -m "Mark as production ready"
      BRANCH_COMMITS[main]+="$(git rev-parse HEAD) "

      MAIN_HEAD=$(git rev-parse HEAD)

      # === DEVELOPMENT BRANCH (7 commits) ===
      git checkout -b development main

      echo "# Development Environment" >> flake.nix
      git add flake.nix
      git commit -q -m "Setup development environment"
      BRANCH_COMMITS[development]+="$(git rev-parse HEAD) "

      echo "# Debug flags enabled" >> flake.nix
      git add flake.nix
      git commit -q -m "Enable debug flags for development"
      BRANCH_COMMITS[development]+="$(git rev-parse HEAD) "

      echo "# Development dependencies" >> flake.nix
      git add flake.nix
      git commit -q -m "Add development dependencies"
      BRANCH_COMMITS[development]+="$(git rev-parse HEAD) "

      echo "# Testing framework integration" >> flake.nix
      git add flake.nix
      git commit -q -m "Integrate testing framework"
      BRANCH_COMMITS[development]+="$(git rev-parse HEAD) "

      echo "# Hot reload support" >> flake.nix
      git add flake.nix
      git commit -q -m "Add hot reload for development"
      BRANCH_COMMITS[development]+="$(git rev-parse HEAD) "

      echo "# Development tools configured" >> flake.nix
      git add flake.nix
      git commit -q -m "Configure development tools"
      BRANCH_COMMITS[development]+="$(git rev-parse HEAD) "

      echo "# Latest development snapshot" >> flake.nix
      git add flake.nix
      git commit -q -m "Development snapshot v1.1.0-dev"
      BRANCH_COMMITS[development]+="$(git rev-parse HEAD) "

      DEV_HEAD=$(git rev-parse HEAD)

      # === FEATURE BRANCH (3 commits) ===
      git checkout -b feature/experimental development

      echo "# Experimental features enabled" >> flake.nix
      git add flake.nix
      git commit -q -m "Enable experimental features"
      BRANCH_COMMITS[feature/experimental]+="$(git rev-parse HEAD) "

      echo "# New algorithm implementation" >> flake.nix
      git add flake.nix
      git commit -q -m "Implement new experimental algorithm"
      BRANCH_COMMITS[feature/experimental]+="$(git rev-parse HEAD) "

      echo "# Performance benchmarks" >> flake.nix
      git add flake.nix
      git commit -q -m "Add performance benchmarks"
      BRANCH_COMMITS[feature/experimental]+="$(git rev-parse HEAD) "

      FEATURE_HEAD=$(git rev-parse HEAD)

      # Switch back to main branch so flake.nix is in the expected state
      git checkout main

      # Output branch information
      echo "$MAIN_HEAD" > MAIN_HEAD
      echo "$DEV_HEAD" > DEVELOPMENT_HEAD
      echo "$FEATURE_HEAD" > FEATURE_HEAD
      echo "''${BRANCH_COMMITS[main]}" | tr ' ' '\n' | grep -v '^$' > MAIN_COMMITS
      echo "''${BRANCH_COMMITS[development]}" | tr ' ' '\n' | grep -v '^$' > DEVELOPMENT_COMMITS
      echo "''${BRANCH_COMMITS[feature/experimental]}" | tr ' ' '\n' | grep -v '^$' > FEATURE_COMMITS
      echo "5" > MAIN_COMMIT_COUNT
      echo "7" > DEVELOPMENT_COMMIT_COUNT
      echo "3" > FEATURE_COMMIT_COUNT
      echo "$MAIN_HEAD" > HEAD_COMMIT
      echo "''${BRANCH_COMMITS[main]}" | tr ' ' '\n' | grep -v '^$' > ALL_COMMITS
      echo "5" > COMMIT_COUNT
    '';

  # Function to create derivation-paths.json
  derivation-paths = {
    pkgs,
    branches ? ["main" "development" "feature/experimental"],
    systems ? ["cf-test-sys" "test-agent"],
    buildAllCommits ? true,
    ...
  }:
    pkgs.runCommand "flake-outputs.json" {
      nativeBuildInputs = [pkgs.nix pkgs.git pkgs.jq];
      flk = testFlake;
      NIX_CONFIG = "experimental-features = nix-command flakes";
      NIXPKGS_PATH = pkgs.path;
    } ''
      set -euo pipefail
      export HOME=$TMPDIR
      git config --global safe.directory "*"
      git config --global user.name "Crystal Forge Test"
      git config --global user.email "test@crystal-forge.dev"

      work="$TMPDIR/work"
      mkdir -p "$work"

      # Return the *evaluated* outPath (no build) for a given checkout/system.
      build_one() {
        local src="$1" sys="$2"
        nix eval --raw \
          --override-input nixpkgs "path:$NIXPKGS_PATH" \
          "$src#nixosConfigurations.''${sys}.config.system.build.toplevel.outPath"
      }

      # (rest of your script unchanged)
      declare -a COMMITS
      if ${
        if buildAllCommits
        then "true"
        else "false"
      }; then
        for br in ${lib.concatStringsSep " " (map (b: "'${b}'") branches)}; do
          file="$flk/$(echo "$br" | tr '/[:lower:]' '_[:upper:]')_COMMITS"
          [ -f "$file" ] || continue
          while IFS= read -r c; do [ -n "$c" ] && COMMITS+=( "$br:$c" ); done < <(tr -d '\r' < "$file")
        done
      else
        for br in ${lib.concatStringsSep " " (map (b: "'${b}'") branches)}; do
          headFile="$flk/$(echo "$br" | tr '/[:lower:]' '_[:upper:]')_HEAD"
          [ -f "$headFile" ] || continue
          COMMITS+=( "$br:$(cat "$headFile")" )
        done
      fi

      echo '{' > "$out"
      first_pair=true
      for pair in "''${COMMITS[@]}"; do
        br="''${pair%%:*}"
        commit="''${pair#*:}"

        src="$work/checkout-$(echo "$br-$commit" | tr -c '[:alnum:]' '-')"
        mkdir -p "$src"
        cp -r "$flk"/. "$src"/
        chmod -R u+w "$src"
        ( cd "$src" && git checkout -q "$commit" && rm -rf .git )

        for sys in ${lib.concatStringsSep " " (map (s: "'${s}'") systems)}; do
          outPath="$(build_one "$src" "$sys")"
          $first_pair || echo ',' >> "$out"
          first_pair=false
          jq -nc \
            --arg key    "''${br}:''${commit}:''${sys}" \
            --arg br     "$br" \
            --arg commit "$commit" \
            --arg system "$sys" \
            --arg out    "$outPath" \
            '{ ($key): { branch:$br, commit:$commit, system:$system, outPath:$out } }' \
            >> "$out"
        done
      done
      echo '}' >> "$out"
    '';

  # Function to preload testFlake at a specific commit into any VM configuration
  # Usage: preloadTestFlake { commit = "abc123"; branch = "main"; path = "/opt/crystal-forge"; }
  #        preloadTestFlake { commitNumber = 3; branch = "main"; }
  preloadTestFlake = {
    commit ? null,
    commitNumber ? null,
    branch ? "main",
    path ? "/opt/crystal-forge",
    ...
  }: let
    # Get the actual commit hash if commitNumber is provided
    actualCommit =
      if commitNumber != null
      then let
        branchCommitsFile = testFlake + "/${lib.strings.toUpper branch}_COMMITS";
        allCommits = lib.strings.splitString "\n" (lib.strings.trim (builtins.readFile branchCommitsFile));
        # commitNumber is 1-indexed, so subtract 1 for list access
        commitIndex = commitNumber - 1;
      in
        if commitIndex >= 0 && commitIndex < (builtins.length allCommits)
        then builtins.elemAt allCommits commitIndex
        else throw "Invalid commitNumber ${toString commitNumber} for branch ${branch}. Valid range: 1-${toString (builtins.length allCommits)}"
      else commit;

    # Create a checkout of testFlake at the specified commit/branch
    flakeCheckout =
      pkgs.runCommand "preloaded-test-flake" {
        nativeBuildInputs = [pkgs.git];
      } ''
                set -euo pipefail
                export HOME=$TMPDIR

                # Configure git to handle repositories safely
                git config --global safe.directory "*"
                git config --global user.name "Crystal Forge Test"
                git config --global user.email "test@crystal-forge.dev"

                # Copy the repository files
                cp -r ${testFlake}/. $out/
                cd $out
                chmod -R u+w .

                # If we have a git repository and need to checkout a specific commit/branch
                if [ -d .git ]; then
                  ${
          if actualCommit != null
          then ''
            git checkout ${actualCommit}
          ''
          else if branch != "main"
          then ''
            git checkout ${branch} || echo "Branch ${branch} not found, staying on current branch"
          ''
          else ""
        }
                  # Remove .git to make it a clean source tree
                  rm -rf .git
                fi

                # Add a marker file with checkout info
                cat > PRELOADED_INFO <<EOF
        Preloaded testFlake
        ${
          if actualCommit != null
          then "Commit: ${actualCommit}"
          else "Branch: ${branch}"
        }
        ${
          if commitNumber != null
          then "Commit Number: ${toString commitNumber}"
          else ""
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

      # Helper script to show available commits
      (pkgs.writeScriptBin "cf-list-commits" ''
        #!${pkgs.bash}/bin/bash
        echo "Available test commits:"
        echo ""

        for branch in main development feature; do
          commits_file="${testFlake}/${lib.strings.toUpper branch}_COMMITS"
          if [ -f "$commits_file" ]; then
            echo "Branch: $branch"
            nl -nln "$commits_file" | sed 's/^/  /'
            echo ""
          fi
        done

        echo "Usage examples:"
        echo "  preloadTestFlake { commitNumber = 3; branch = \"main\"; }"
        echo "  preloadTestFlake { commitNumber = 1; branch = \"development\"; }"
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
    preloadTestFlake # Function to preload testFlake into any VM
    ;
}
