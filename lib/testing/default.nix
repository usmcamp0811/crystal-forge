# Crystal Forge Test Library Functions
#
# This library provides utilities for creating offline flake tests by:
# 1. Pre-downloading all flake dependencies from flake.lock
# 2. Creating a git repository from the flake source
# 3. Setting up registry mappings for offline dependency resolution
#
# Usage: with lib.crystal-forge; { testFlake = ...; prefetchedPaths = ...; }
{
  lib,
  inputs,
  system ? null,
  ...
}: let
  # Default to x86_64-linux if no system specified
  actualSystem =
    if system != null
    then system
    else "x86_64-linux";

  pkgs = inputs.nixpkgs.legacyPackages.${actualSystem};

  # Use a fixed, eval-time path for the flake source
  # This avoids reading from a derivation output during evaluation
  srcPath = builtins.path {
    path = ../../.;
    name = "flake-src";
  };

  # Parse the flake.lock file to get dependency information
  lockJson = builtins.fromJSON (builtins.readFile (srcPath + "/flake.lock"));
  nodes = lockJson.nodes;

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
  # Create a git repository from the flake source for testing
  # This simulates a real git-tracked flake environment
  testFlake = pkgs.stdenv.mkDerivation {
    name = "flake-as-git";
    src = srcPath;
    nativeBuildInputs = [pkgs.git];
    buildPhase = ''
      mkdir -p "$out"
      cp -r "$src"/. "$out/"
      cd "$out"
      git init -q
      git config user.name "Nix Build"
      git config user.email "nix@build.local"
      git add -A
      git commit -q -m "Packaged flake for testing"
    '';
    installPhase = "true";
  };

  # Export all the computed values for use in tests
  inherit
    lockJson # Parsed flake.lock content
    nodes # Dependency nodes from flake.lock
    prefetchedList # List of { key, path, from } records
    prefetchedPaths # List of Nix store paths for dependencies
    registryEntries
    ; # Registry mappings for offline resolution
}
