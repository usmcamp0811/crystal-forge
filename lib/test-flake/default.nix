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
in rec {
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

  inherit
    lockJson # Parsed flake.lock content
    nodes # Dependency nodes from flake.lock
    prefetchedList # List of { key, path, from } records
    prefetchedPaths # List of Nix store paths for dependencies
    registryEntries
    ; # Registry mappings for offline resolution
}
