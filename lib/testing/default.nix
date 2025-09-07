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

      git add -A
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
        before = ["git-daemon.service"];
        wantedBy = ["multi-user.target"];

        serviceConfig = {
          Type = "oneshot";
          User = "git";
          Group = "git";
          ExecStart = "${pkgs.bash}/bin/bash -c 'cp -r ${testFlake} /srv/git/crystal-forge.git && chown -R git:git /srv/git/crystal-forge.git && chmod -R u+w /srv/git/crystal-forge.git'";
          RemainAfterExit = true; # Important: keeps the service "active" after completion
        };
      };

      environment.etc."gitconfig".text = ''
        [safe]
            directory = /srv/git/crystal-forge.git
      '';

      # Configure cgit web interface with better service dependencies
      services.cgit = {
        gitserver = {
          enable = true;
          scanPath = "/srv/git";

          settings = {
            root-title = "Crystal Forge Git Server";
            root-desc = "Test Git repositories for Crystal Forge";
            enable-follow-links = true;
            enable-index-links = true;
            enable-log-filecount = true;
            enable-log-linecount = true;
            source-filter = "${pkgs.cgit}/lib/cgit/filters/syntax-highlighting.py";
            about-filter = "${pkgs.cgit}/lib/cgit/filters/about-formatting.sh";
            cache-size = 1000;
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

      # Override the fcgiwrap service to wait for git repo setup
      systemd.services.fcgiwrap-cgit-gitserver = {
        after = ["setup-git-repo.service"];
        wants = ["setup-git-repo.service"];

        # Add restart on failure with delay
        serviceConfig = {
          Restart = "on-failure";
          RestartSec = "5s";
          StartLimitBurst = 3;
          StartLimitIntervalSec = "30s";
        };
      };

      # Enable nginx for cgit
      services.nginx.enable = true;

      # Git daemon for git:// protocol access
      systemd.services.git-daemon = {
        enable = true;
        description = "Git Daemon for git:// protocol";
        after = ["network.target" "setup-git-repo.service"];
        wants = ["setup-git-repo.service"];
        wantedBy = ["multi-user.target"];

        serviceConfig = {
          Type = "exec";
          User = "git";
          Group = "git";
          WorkingDirectory = "/srv/git";
          ExecStart = "${pkgs.git}/bin/git daemon --verbose --export-all --base-path=/srv/git --reuseaddr --port=${toString port}";
          Environment = "HOME=/srv/git";
          Restart = "on-failure";
          RestartSec = "5s";
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
      virtualisation.memorySize = 8096;
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
              initial_commit_depth = 5;
            }
            # {
            #   name = "crystal-forge-development";
            #   repo_url = "http://gitserver/crystal-forge?ref=development";
            #   auto_poll = true;
            #   initial_commit_depth = 7;
            # }
            # {
            #   name = "crystal-forge-feature";
            #   repo_url = "http://gitserver/crystal-forge?ref=feature/experimental";
            #   auto_poll = true;
            #   initial_commit_depth = 3;
            # }
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
