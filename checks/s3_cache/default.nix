{
  lib,
  inputs,
  pkgs,
  ...
}: let
  keyPair = pkgs.runCommand "agent-keypair" {} ''
    mkdir -p $out
    ${pkgs.crystal-forge.agent.cf-keygen}/bin/cf-keygen -f $out/agent.key
  '';
  keyPath = pkgs.runCommand "agent.key" {} ''
    mkdir -p $out
    cp ${keyPair}/agent.key $out/
  '';
  pubPath = pkgs.runCommand "agent.pub" {} ''
    mkdir -p $out
    cp ${keyPair}/agent.pub $out/
  '';
  derivation-paths = lib.crystal-forge.derivation-paths pkgs;
  CF_TEST_DB_PORT = 5432;
  CF_TEST_SERVER_PORT = 3000;
  systemBuildClosure = pkgs.closureInfo {
    rootPaths =
      [
        inputs.self.nixosConfigurations.cf-test-sys.config.system.build.toplevel
        pkgs.crystal-forge.default
        pkgs.path
      ]
      ++ lib.crystal-forge.prefetchedPaths;
  };
in
  pkgs.testers.runNixOSTest {
    name = "crystal-forge-s3-cache-integration";
    skipLint = true;
    skipTypeCheck = true;
    nodes = {
      gitserver = lib.crystal-forge.makeGitServerNode {
        inherit pkgs systemBuildClosure;
        port = 8080;
      };

      s3Cache = lib.crystal-forge.makeS3CacheNode {
        inherit pkgs;
        bucketName = "crystal-forge-cache";
        port = 9000;
        consolePort = 9001;
      };

      s3Server = {
        imports = [inputs.self.nixosModules.crystal-forge];

        networking.useDHCP = true;
        networking.firewall.allowedTCPPorts = [CF_TEST_SERVER_PORT 5432];

        virtualisation.writableStore = true;
        virtualisation.memorySize = 8096;
        virtualisation.cores = 8;
        virtualisation.additionalPaths = [
          systemBuildClosure
          inputs.self.nixosConfigurations.cf-test-sys.config.system.build.toplevel.drvPath
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
          awscli2
          curl
          crystal-forge.default
          crystal-forge.cf-test-modules.runTests
          crystal-forge.cf-test-modules.testRunner
        ];

        environment.etc = {
          "agent.key".source = "${keyPath}/agent.key";
          "agent.pub".source = "${pubPath}/agent.pub";
        };

        services.crystal-forge = {
          enable = true;
          "local-database" = true;
          log_level = "debug";
          client.enable = false;

          # Database config
          database = {
            host = "localhost";
            user = "crystal_forge";
            name = "crystal_forge";
            port = 5432;
          };

          # Server config
          server = {
            port = CF_TEST_SERVER_PORT;
            enable = false;
            host = "0.0.0.0";
          };

          # Build configuration with S3 environment variables
          build = {
            enable = true;
            offline = false;
            systemd_properties = [
              # "Environment=AWS_ENDPOINT_URL=http://s3Cache:9000"
              "Environment=AWS_ACCESS_KEY_ID=minioadmin"
              "Environment=AWS_SECRET_ACCESS_KEY=minioadmin"
              "Environment=AWS_REGION=us-east-1" # ADD THIS LINE
              "Environment=AWS_EC2_METADATA_DISABLED=true" # ADD THIS LINE TOO
              "Environment=NIX_LOG=trace"
              "Environment=NIX_SHOW_STATS=1"
            ];
          };

          # S3 cache configuration
          cache = {
            cache_type = "S3";
            push_to = "s3://crystal-forge-cache?endpoint=http://s3Cache:9000&scheme=http&region=us-east-1&force-path-style=true";
            push_after_build = true;
            s3_region = "us-east-1";
            parallel_uploads = 2;
            max_retries = 2;
            retry_delay_seconds = 1;
          };

          # Test flake configuration - this is what the test expects
          flakes = {
            flake_polling_interval = "1m";
            watched = [
              # {
              #   name = "test-flake";
              #   repo_url = "http://gitserver/crystal-forge";
              #   auto_poll = true;
              #   initial_commit_depth = 5;
              # }
            ];
          };

          # Test environment
          environments = [
            # {
            #   name = "test";
            #   description = "Test environment for Crystal Forge agents and evaluation";
            #   is_active = true;
            #   risk_profile = "LOW";
            #   compliance_level = "NONE";
            # }
          ];

          # Test system configuration
          systems = [
            # {
            #   hostname = "agent";
            #   public_key = lib.strings.trim (builtins.readFile "${pubPath}/agent.pub");
            #   environment = "test";
            #   flake_name = "test-flake";
            # }
          ];
        };
      };
    };

    globalTimeout = 300; # 5 minutes
    extraPythonPackages = p: [p.pytest pkgs.crystal-forge.vm-test-logger pkgs.crystal-forge.cf-test-modules];

    testScript = ''
      import os
      import pytest

      os.environ["NIXOS_TEST_DRIVER"] = "1"
      start_all()

      # Wait for S3 cache service
      s3Cache.wait_for_unit("minio.service")
      s3Cache.wait_for_unit("minio-setup.service")
      s3Cache.wait_for_open_port(9000)

      # Get s3Cache IP address and set it as an environment variable
      s3_cache_ip = s3Cache.succeed("ip -4 addr show eth0 | grep -oP '(?<=inet\\s)\\d+(\\.\\d+){3}'").strip()
      s3Server.log(f"s3Cache IP: {s3_cache_ip}")

      # Update the environment variable to use IP instead of hostname
      s3Server.succeed(f"systemctl set-environment AWS_ENDPOINT_URL=http://{s3_cache_ip}:9000")
      s3Server.succeed("systemctl daemon-reload")
      # s3Server.succeed("systemctl restart crystal-forge-builder.service")

      # Test that s3Server can reach s3Cache
      s3Server.succeed("ping -c 1 s3Cache")
      s3Server.succeed("curl -f http://s3Cache:9000/minio/health/live")
      # Test direct S3 connection from s3Server
      s3Server.log("Testing direct S3 connection...")

      # First, create a simple test file to push
      s3Server.succeed("echo 'test content' > /tmp/test-file")
      s3Server.succeed("nix-store --add /tmp/test-file")

      # Get the store path of the test file
      test_store_path = "${inputs.self.nixosConfigurations.cf-test-sys.config.system.build.toplevel.drvPath}"
      s3Server.log(f"Test store path: {test_store_path}")

      # Wait for S3 server services
      s3Server.wait_for_unit("postgresql.service")
      s3Server.wait_for_unit("crystal-forge-builder.service")
      s3Server.wait_for_open_port(5432)
      s3Server.forward_port(5433, 5432)

      s3Server.succeed("systemctl list-unit-files | grep crystal-forge")

      try:
          s3Server.succeed("systemctl start crystal-forge-builder.service")
          s3Server.wait_for_unit("crystal-forge-builder.service")
          s3Server.log("✅ Builder service started successfully")
      except:
          s3Server.log("⚠️ Builder service not available or failed to start")
          s3Server.succeed("systemctl status crystal-forge-server.service")

      from cf_test.vm_helpers import wait_for_git_server_ready
      wait_for_git_server_ready(gitserver, timeout=60)

      # --- Added environment variables for completed_derivation_data ---
      os.environ["CF_TEST_PACKAGE_DRV"] = "${inputs.self.nixosConfigurations.cf-test-sys.config.system.build.toplevel.drvPath}"
      os.environ["CF_TEST_PACKAGE_NAME"] = "cf-test-sys"
      os.environ["CF_TEST_PACKAGE_VERSION"] = "0.1.0"
      # -----------------------------------------------------------------

      os.environ["CF_TEST_GIT_SERVER_URL"] = "http://gitserver/crystal-forge"
      os.environ["CF_TEST_DB_HOST"] = "127.0.0.1"
      os.environ["CF_TEST_DB_PORT"] = "5433"
      os.environ["CF_TEST_DB_USER"] = "postgres"
      os.environ["CF_TEST_DB_PASSWORD"] = ""

      os.environ["CF_TEST_SERVER_HOST"] = "127.0.0.1"
      os.environ["CF_TEST_SERVER_PORT"] = "${toString CF_TEST_SERVER_PORT}"

      os.environ["CF_TEST_DRV"] = "${derivation-paths}"

      import cf_test
      cf_test._driver_machines = {
          "s3Server": s3Server,
          "s3Cache": s3Cache,
          "gitserver": gitserver,
      }

      exit_code = pytest.main([
          "-vvvv", "--tb=short", "-x", "-s", "-m", "s3cache", "--pyargs", "cf_test",
      ])
      if exit_code != 0:
          raise SystemExit(exit_code)
    '';
  }
