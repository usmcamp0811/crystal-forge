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
  srcPath = builtins.fetchGit {
    url = "https://gitlab.com/crystal-forge/crystal-forge.git";
    rev = "f155b4ec2f706828d75dab9c4b7ff3a891bdd3d2";
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

      # Create development history with multiple commits
      git add -f flake.nix
      [ -f flake.lock ] && git add -f flake.lock
      git commit -q -m "Initial flake configuration"

      echo "# Crystal Forge Test Environment" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Add documentation comment"

      git add -A
      git commit -q -m "Add remaining project files"

      echo "" >> flake.nix
      echo "# Last updated: $(date)" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Update timestamp"

      # Create bare repository for serving
      git init --bare "$out"
      git -C "$out" config receive.denyCurrentBranch ignore
      git push "$out" HEAD:refs/heads/main
      git -C "$out" symbolic-ref HEAD refs/heads/main

      # Enable Git HTTP backend
      git -C "$out" config http.receivepack true
      git -C "$out" config http.uploadpack true
      git -C "$out" update-server-info
    '';

  # Create a reusable git server node for tests
  # This provides a standardized git server that can serve repositories over git protocol
  makeGitServerNode = {
    port ? 8080,
    extraConfig ? {},
    systemBuildClosure,
    pkgs,
  }: {
    services.getty.autologinUser = "root";
    networking.firewall.allowedTCPPorts = [port];
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

    # Create git user for proper permissions
    users.users.git = {
      isSystemUser = true;
      group = "git";
      home = "/srv/git";
      createHome = true;
    };
    users.groups.git = {};

    systemd.tmpfiles.rules = [
      "d /srv/git 0755 git git -"
      "L+ /srv/git/crystal-forge.git - - - - ${testFlake}"
    ];

    environment.etc."gitconfig".text = ''
      [safe]
          directory = /srv/git/crystal-forge.git
    '';

    systemd.services.git-http-server = {
      enable = true;
      description = "Git HTTP Server for Crystal Forge";
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

    systemd.services.fix-git-ownership = {
      enable = true;
      description = "Fix Git Repository Ownership";
      after = ["systemd-tmpfiles-setup.service"];
      before = ["git-http-server.service"];
      wantedBy = ["multi-user.target"];

      serviceConfig = {
        Type = "oneshot";
        ExecStart = "${pkgs.bash}/bin/bash -c 'chown -R git:git /srv/git/crystal-forge.git'";
      };
    };
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
