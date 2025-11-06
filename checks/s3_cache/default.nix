{
  lib,
  inputs,
  pkgs,
  ...
}: let
  keyPair = pkgs.runCommand "agent-keypair" {} ''
    mkdir -p $out
    ${pkgs.crystal-forge.cf-keygen}/bin/cf-keygen -f $out/agent.key
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

      cfServer = {
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
          crystal-forge.cf-test-suite.runTests
          crystal-forge.cf-test-suite.testRunner
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
    extraPythonPackages = p: [p.pytest  pkgs.crystal-forge.cf-test-suite];

    testScript = ''
      import os
      import pytest

      # Set test-specific environment variables
      os.environ.update({
          "CF_TEST_PACKAGE_DRV": "${inputs.self.nixosConfigurations.cf-test-sys.config.system.build.toplevel.drvPath}",
          "CF_TEST_PACKAGE_NAME": "cf-test-sys",
          "CF_TEST_PACKAGE_VERSION": "0.1.0",
          "CF_TEST_SERVER_PORT": "${toString CF_TEST_SERVER_PORT}",
          "CF_TEST_DRV": "${derivation-paths}",
      })

      # Configure machine access for cf_test
      import cf_test
      cf_test._driver_machines = {
          "cfServer": cfServer,  # Note: using cfServer variable, mapping to "cfServer" key
          "s3Cache": s3Cache,
          "gitserver": gitserver,
      }

      # Run the s3cache tests
      exit_code = pytest.main([
          "-vvvv", "--tb=short", "-x", "-s",
          "-m", "s3cache", "--pyargs", "cf_test",
      ])

      if exit_code != 0:
          raise SystemExit(exit_code)
    '';
  }
