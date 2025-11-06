{
  lib,
  inputs,
  pkgs,
  ...
}: let
  keyPair = pkgs.runCommand "agent-keypair" {} ''
    mkdir -p $out
    ${pkgs.crystal-forge.default.cf-keygen}/bin/cf-keygen -f $out/agent.key
  '';
  keyPath = pkgs.runCommand "agent.key" {} ''
    mkdir -p $out
    cp ${keyPair}/agent.key $out/
  '';
  pubPath = pkgs.runCommand "agent.pub" {} ''
    mkdir -p $out
    cp ${keyPair}/agent.pub $out/
  '';
  
  CF_SERVER_PORT = 3000;
  GIT_SERVER_PORT = 8080;
  
  systemBuildClosure = pkgs.closureInfo {
    rootPaths = [
      inputs.self.nixosConfigurations.cf-test-sys.config.system.build.toplevel
      pkgs.crystal-forge.default
      pkgs.path
    ] ++ lib.crystal-forge.prefetchedPaths;
  };
in
  pkgs.testers.runNixOSTest {
    name = "crystal-forge-dev-env";
    skipLint = true;
    skipTypeCheck = true;
    
    nodes = {
      gitserver = lib.crystal-forge.makeGitServerNode {
        inherit pkgs systemBuildClosure;
        port = GIT_SERVER_PORT;
      };

      server = {
        nix.settings = {
          experimental-features = ["nix-command" "flakes"];
          use-registries = false;
        };
        imports = [inputs.self.nixosModules.crystal-forge];

        networking.useDHCP = true;
        networking.firewall.allowedTCPPorts = [CF_SERVER_PORT 5432];

        virtualisation.writableStore = true;
        virtualisation.memorySize = 6144;
        virtualisation.cores = 4;
        virtualisation.diskSize = 16384;
        virtualisation.additionalPaths = [
          systemBuildClosure
          inputs.self.nixosConfigurations.cf-test-sys.config.system.build.toplevel.drvPath
        ];

        systemd.tmpfiles.rules = [
          "d /var/lib/crystal-forge 0755 crystal-forge crystal-forge -"
          "d /var/lib/crystal-forge/.cache 0755 crystal-forge crystal-forge -"
          "d /var/lib/crystal-forge/.cache/nix 0755 crystal-forge crystal-forge -"
        ];

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

        environment.systemPackages = with pkgs; [
          git
          jq
          hello
          curl
          crystal-forge.default
          # Development tools
          postgresql
          vim
          htop
          tree
        ];

        environment.variables = {
          TMPDIR = "/tmp";
          TMP = "/tmp";
          TEMP = "/tmp";
        };

        environment.etc = {
          "agent.key".source = "${keyPath}/agent.key";
          "agent.pub".source = "${pubPath}/agent.pub";
        };

        services.crystal-forge = {
          enable = true;
          local-database = true;
          log_level = "debug";
          
          client = {
            enable = true;
            server_host = "server";
            server_port = CF_SERVER_PORT;
            private_key = "/etc/agent.key";
          };

          database = {
            host = "localhost";
            user = "crystal_forge";
            name = "crystal_forge";
            port = 5432;
          };

          server = {
            enable = true;
            port = CF_SERVER_PORT;
            host = "0.0.0.0";
          };

          build = {
            enable = false;
            offline = true;
          };

          flakes = {
            flake_polling_interval = "1m";
            watched = [
              {
                name = "test-flake";
                repo_url = "http://gitserver/crystal-forge";
                auto_poll = true;
                initial_commit_depth = 5;
              }
            ];
          };

          environments = [
            {
              name = "development";
              description = "Development environment for Crystal Forge";
              is_active = true;
              risk_profile = "LOW";
              compliance_level = "NONE";
            }
          ];

          systems = [
            {
              hostname = "cf-dev-sys";
              public_key = lib.strings.trim (builtins.readFile "${pubPath}/agent.pub");
              environment = "development";
              flake_name = "test-flake";
            }
          ];
        };
      };
    };

    globalTimeout = 900; # 15 minutes for interactive use

    testScript = ''
      # Start all VMs
      start_all()

      # Wait for services to be ready
      print("Waiting for PostgreSQL...")
      server.wait_for_unit("postgresql.service")
      
      print("Waiting for Crystal Forge server...")
      server.wait_for_unit("crystal-forge-server.service")
      server.wait_for_open_port(5432)
      server.wait_for_open_port(${toString CF_SERVER_PORT})

      # Forward ports so you can access from host
      print("Setting up port forwards...")
      server.forward_port(5433, 5432)  # PostgreSQL
      server.forward_port(${toString CF_SERVER_PORT}, ${toString CF_SERVER_PORT})  # Crystal Forge API
      server.forward_port(${toString GIT_SERVER_PORT}, ${toString GIT_SERVER_PORT})  # Git server (if needed)

      print("=" * 60)
      print("Crystal Forge Development Environment Ready!")
      print("=" * 60)
      print(f"Crystal Forge API: http://localhost:${toString CF_SERVER_PORT}")
      print(f"PostgreSQL: localhost:5433")
      print(f"Git Server: http://localhost:${toString GIT_SERVER_PORT}")
      print("")
      print("Available commands:")
      print("  server.shell_interact()     - Get shell on main server")
      print("  gitserver.shell_interact()  - Get shell on git server")
      print("  server.succeed('command')   - Run command on server")
      print("  server.fail('command')      - Expect command to fail")
      print("=" * 60)
    '';
  }
