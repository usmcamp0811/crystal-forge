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

      # Array to store commit hashes
      COMMIT_HASHES=()

      # Create development history with 15 commits
      git add -f flake.nix
      [ -f flake.lock ] && git add -f flake.lock
      git commit -q -m "Initial flake configuration"
      COMMIT_HASHES+=($(git rev-parse HEAD))

      echo "# Crystal Forge Test Environment" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Add documentation comment"
      COMMIT_HASHES+=($(git rev-parse HEAD))

      git add -A
      git commit -q -m "Add remaining project files"
      COMMIT_HASHES+=($(git rev-parse HEAD))

      echo "" >> flake.nix
      echo "# Last updated: $(date)" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Update timestamp"
      COMMIT_HASHES+=($(git rev-parse HEAD))

      echo "# Version: 1.0.0" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Add version information"
      COMMIT_HASHES+=($(git rev-parse HEAD))

      echo "# Maintainer: Crystal Forge Team" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Add maintainer information"
      COMMIT_HASHES+=($(git rev-parse HEAD))

      echo "# License: MIT" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Add license information"
      COMMIT_HASHES+=($(git rev-parse HEAD))

      echo "# Dependencies updated" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Update dependencies"
      COMMIT_HASHES+=($(git rev-parse HEAD))

      echo "# Performance improvements" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Performance optimizations"
      COMMIT_HASHES+=($(git rev-parse HEAD))

      echo "# Security fixes applied" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Apply security patches"
      COMMIT_HASHES+=($(git rev-parse HEAD))

      echo "# Documentation updates" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Update documentation"
      COMMIT_HASHES+=($(git rev-parse HEAD))

      echo "# Build system improvements" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Improve build system"
      COMMIT_HASHES+=($(git rev-parse HEAD))

      echo "# Testing framework added" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Add testing framework"
      COMMIT_HASHES+=($(git rev-parse HEAD))

      echo "# CI/CD pipeline configured" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Configure CI/CD pipeline"
      COMMIT_HASHES+=($(git rev-parse HEAD))

      echo "# Final release preparation" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Prepare for release"
      COMMIT_HASHES+=($(git rev-parse HEAD))

      # Get the final commit hash BEFORE creating bare repo
      FINAL_COMMIT=$(git rev-parse HEAD)

      # Create bare repository for serving
      git init --bare "$out"
      git -C "$out" config receive.denyCurrentBranch ignore
      git push "$out" HEAD:refs/heads/main
      git -C "$out" symbolic-ref HEAD refs/heads/main

      # Enable Git HTTP backend
      git -C "$out" config http.receivepack true
      git -C "$out" config http.uploadpack true
      git -C "$out" update-server-info

      # Output the commit hash to a file in the bare repo
      echo "$FINAL_COMMIT" > "$out/HEAD_COMMIT"

      # Output all commit hashes (one per line)
      printf "%s\n" "''${COMMIT_HASHES[@]}" > "$out/ALL_COMMITS"

      # Output count of commits
      echo "''${#COMMIT_HASHES[@]}" > "$out/COMMIT_COUNT"
    '';

  # Create a reusable git server node for tests with cgit web interface
  # This provides a standardized git server that can serve repositories over git protocol and HTTP
  makeGitServerNode = {
    port ? 8080,
    extraConfig ? {},
    systemBuildClosure,
    pkgs,
  }:
    {
      services.getty.autologinUser = "root";
      networking.firewall.allowedTCPPorts = [port 80];
      virtualisation.writableStore = true;
      virtualisation.memorySize = 2048;
      virtualisation.additionalPaths = [systemBuildClosure];

      environment.systemPackages = [pkgs.git pkgs.jq pkgs.crystal-forge.cf-test-modules.runTests pkgs.crystal-forge.cf-test-modules.testRunner];

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

      # Create cgit user
      users.users.cgit = {
        isSystemUser = true;
        group = "cgit";
        home = "/var/lib/cgit";
      };
      users.groups.cgit = {};

      systemd.tmpfiles.rules = [
        "d /srv/git 0755 git git -"
        "d /var/lib/cgit 0755 cgit cgit -"
      ];

      # Service to create writable git repository
      systemd.services.setup-git-repo = {
        enable = true;
        description = "Setup writable git repository";
        after = ["systemd-tmpfiles-setup.service"];
        before = ["git-daemon.service" "cgit-gitserver.service"];
        wantedBy = ["multi-user.target"];

        serviceConfig = {
          Type = "oneshot";
          User = "git";
          Group = "git";
          ExecStart = "${pkgs.bash}/bin/bash -c 'cp -r ${testFlake} /srv/git/crystal-forge.git && chown -R git:git /srv/git/crystal-forge.git && chmod -R u+w /srv/git/crystal-forge.git'";
        };
      };

      environment.etc."gitconfig".text = ''
        [safe]
            directory = /srv/git/crystal-forge.git
      '';

      # Configure cgit web interface
      services.cgit = {
        gitserver = {
          enable = true;

          # Use scanPath to automatically discover repositories
          scanPath = "/srv/git";

          settings = {
            # Basic appearance
            root-title = "Crystal Forge Git Server";
            root-desc = "Test Git repositories for Crystal Forge";

            # Enable features
            enable-follow-links = true;
            enable-index-links = true;
            enable-log-filecount = true;
            enable-log-linecount = true;

            # Syntax highlighting
            source-filter = "${pkgs.cgit}/lib/cgit/filters/syntax-highlighting.py";
            about-filter = "${pkgs.cgit}/lib/cgit/filters/about-formatting.sh";

            # Cache for performance
            cache-size = 1000;

            # Allow cloning
            enable-git-config = true;
          };

          nginx = {
            virtualHost = "localhost";
            location = "/";
          };

          user = "cgit";
          group = "cgit";
        };
      };

      # Enable nginx for cgit
      services.nginx.enable = true;

      # Git daemon for git:// protocol access
      systemd.services.git-daemon = {
        enable = true;
        description = "Git Daemon for git:// protocol";
        after = ["network.target"];
        wantedBy = ["multi-user.target"];

        serviceConfig = {
          Type = "exec";
          User = "git";
          Group = "git";
          WorkingDirectory = "/srv/git";
          ExecStart = "${pkgs.git}/bin/git daemon --verbose --export-all --base-path=/srv/git --reuseaddr --port=${toString port}";
          Environment = "HOME=/srv/git";
        };
      };
    }
    // extraConfig;

  makeServerNode = {
    pkgs,
    systemBuildClosure,
    keyPath ? null,
    pubPath ? null,
    cfFlakePath ? null,
    port ? 3000,
    agents ? [],
    extraConfig ? {},
    ...
  }: let
    # Generate keypairs for agents if not provided
    agentKeyPairs =
      map (agentName: {
        name = agentName;
        keyPair = lib.crystal-forge.mkKeyPair {
          inherit pkgs;
          name = agentName;
        };
      })
      agents;

    # Generate systems configuration from agents
    agentSystems =
      map (agent: {
        hostname = agent.name;
        public_key = lib.crystal-forge.mkPublicKey {
          inherit pkgs;
          name = agent.name;
          keyPair = agent.keyPair;
        };
        environment = "test";
        flake_name = "dotfiles";
      })
      agentKeyPairs;
  in
    {
      networking.useDHCP = true;
      networking.firewall.allowedTCPPorts = [port 5432];
      virtualisation.writableStore = true;
      virtualisation.memorySize = 4096;
      virtualisation.additionalPaths = [systemBuildClosure];

      environment.systemPackages = [pkgs.git pkgs.jq pkgs.crystal-forge.default pkgs.crystal-forge.cf-test-modules.runTests pkgs.crystal-forge.cf-test-modules.testRunner];
      environment.etc = lib.mkMerge [
        (lib.mkIf (keyPath != null) {"agent.key".source = "${keyPath}/agent.key";})
        (lib.mkIf (pubPath != null) {"agent.pub".source = "${pubPath}/agent.pub";})
        (lib.mkIf (cfFlakePath != null) {"cf_flake".source = cfFlakePath;})
      ];
      environment.variables = {
        PGHOST = "/run/postgresql";
        PGUSER = "postgres";
      };

      services.postgresql = {
        enable = true;
        settings."listen_addresses" = lib.mkForce "*";
        authentication = lib.concatStringsSep "\n" [
          "local   all   postgres   trust"
          "local   all   all        peer"
          "host    all   all 127.0.0.1/32 trust"
          "host    all   all ::1/128      trust"
          "host    all   all 10.0.2.2/32  trust"
        ];
        initialScript = pkgs.writeText "init-crystal-forge.sql" ''
          CREATE USER crystal_forge LOGIN;
          CREATE DATABASE crystal_forge OWNER crystal_forge;
          GRANT ALL PRIVILEGES ON DATABASE crystal_forge TO crystal_forge;
        '';
      };

      services.crystal-forge = lib.mkMerge [
        {
          enable = true;
          local-database = true;
          log_level = "debug";
          build.offline = true;
          database = {
            user = "crystal_forge";
            host = "localhost";
            name = "crystal_forge";
          };
          flakes.flake_polling_interval = "1m";
          flakes.watched = [
            {
              name = "crystal-forge";
              repo_url = "http://gitserver/crystal-forge";
              auto_poll = true;
            }
          ];
          environments = [
            {
              name = "test";
              description = "Test environment for Crystal Forge agents and evaluation";
              is_active = true;
              risk_profile = "LOW";
              compliance_level = "NONE";
            }
          ];
          server = {
            enable = true;
            host = "0.0.0.0";
            port = port;
          };
        }
        # Use generated agent systems if agents provided
        (lib.mkIf (agents != []) {
          systems = agentSystems;
        })
        # Fallback to single agent if pubPath provided and no agents list
        (lib.mkIf (pubPath != null && agents == []) {
          systems = [
            {
              hostname = "agent";
              public_key = lib.strings.trim (builtins.readFile "${pubPath}/agent.pub");
              environment = "test";
              flake_name = "crystal-forge";
            }
          ];
        })
      ];
    }
    // extraConfig;

  makeAgentNode = {
    pkgs,
    keyPath,
    pubPath,
    systemBuildClosure,
    serverHost ? "server",
    serverPort ? 3000,
    enableFirewall ? false,
    extraConfig ? {},
    ...
  }:
    {
      virtualisation.writableStore = true;
      virtualisation.memorySize = 2048;
      virtualisation.additionalPaths = [systemBuildClosure];

      environment.systemPackages = [pkgs.git pkgs.jq pkgs.crystal-forge.cf-test-modules.runTests pkgs.crystal-forge.cf-test-modules.testRunner];
      networking.useDHCP = true;
      networking.firewall.enable = false;

      environment.etc."agent.key".source = "${keyPath}/agent.key";
      environment.etc."agent.pub".source = "${pubPath}/agent.pub";

      services.crystal-forge = {
        enable = true;
        client = {
          enable = true;
          server_host = "server";
          server_port = 3000;
          private_key = "/etc/agent.key";
        };
      };
    }
    // extraConfig;

  # Export all the computed values for use in tests
  inherit
    lockJson # Parsed flake.lock content
    nodes # Dependency nodes from flake.lock
    prefetchedList # List of { key, path, from } records
    prefetchedPaths # List of Nix store paths for dependencies
    registryEntries
    ; # Registry mappings for offline resolution
}
