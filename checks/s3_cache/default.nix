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
  testFlakeCommitHash = pkgs.runCommand "test-flake-commit" {} ''
    cat ${lib.crystal-forge.testFlake}/HEAD_COMMIT > $out
  '';

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

      s3Server = lib.crystal-forge.makeServerNode {
        inherit inputs pkgs systemBuildClosure keyPath pubPath;
        extraConfig = {
          services.crystal-forge = {
            cache = {
              cache_type = "S3";
              push_to = "s3://crystal-forge-cache";
              push_after_build = true;
              s3_region = "us-east-1";
              parallel_uploads = 2;
              max_retries = 2;
              retry_delay_seconds = 1;
            };
            build = {
              enable = true;
              systemd_properties = [
                "Environment=AWS_ENDPOINT_URL=http://s3Cache:9000"
                "Environment=AWS_ACCESS_KEY_ID=minioadmin"
                "Environment=AWS_SECRET_ACCESS_KEY=minioadmin"
              ];
            };
            # log_level = "debug";
            flakes.watched = [
              {
                name = "test-flake";
                repo_url = "http://gitserver/crystal-forge";
                auto_poll = true;
              }
            ];

            environments = [
              {
                name = "test";
                description = "Computers that get on wifi";
                is_active = true;
                risk_profile = "MEDIUM";
                compliance_level = "NONE";
              }
            ];
            # Updated systems configuration (new requirement)
            systems = [
            ];

            server = {
              enable = true;
              host = "0.0.0.0"; # Added explicit host
              port = CF_TEST_SERVER_PORT; # Kept your custom port
              # Note: authorized_keys moved to systems configuration above
            };

            # Database configuration (using defaults)
            database = {
              host = "localhost";
              user = "crystal_forge";
              name = "crystal_forge";
              port = 5432;
            };
          };
        };
        port = CF_TEST_SERVER_PORT;
      };
    };

    globalTimeout = 200; # 20 minutes for cache operations
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

      # Wait for S3 server
      s3Server.wait_for_unit("postgresql.service")
      s3Server.wait_for_unit("crystal-forge-server.service")
      s3Server.succeed("systemctl list-unit-files | grep crystal-forge")
      s3Server.succeed("systemctl restart crystal-forge-builder")
      s3Server.succeed("ls -la /etc/systemd/system/crystal-forge*")
      s3Server.wait_for_unit("crystal-forge-builder.service")
      s3Server.wait_for_open_port(5432)
      s3Server.forward_port(5433, 5432)


      from cf_test.vm_helpers import wait_for_git_server_ready
      wait_for_git_server_ready(gitserver, timeout=120)

      # Set up test environment variables
      os.environ["CF_TEST_GIT_SERVER_URL"] = "http://gitserver/crystal-forge"
      os.environ["CF_TEST_REAL_COMMIT_HASH"] = "${testFlakeCommitHash}"

      # Set environment variables for the test
      os.environ["CF_TEST_DB_HOST"] = "127.0.0.1"
      os.environ["CF_TEST_DB_PORT"] = "5433"
      os.environ["CF_TEST_DB_USER"] = "postgres"
      os.environ["CF_TEST_DB_PASSWORD"] = ""  # no password for VM postgres
      os.environ["CF_TEST_SERVER_HOST"] = "127.0.0.1"
      os.environ["CF_TEST_SERVER_PORT"] = "${toString CF_TEST_SERVER_PORT}"

      # Legacy single-branch data (main branch)
      os.environ["CF_TEST_REAL_COMMIT_HASH"] = "${testFlakeCommitHash}"
      os.environ["CF_TEST_REAL_REPO_URL"] = "http://gitserver/crystal-forge"

      # Inject machines for test access
      import cf_test
      cf_test._driver_machines = {
          "s3Server": s3Server,
          "s3Cache": s3Cache,
          "gitserver": gitserver,
      }

      # Run S3 cache-specific tests
      exit_code = pytest.main([
          "-vvvv",
          "--tb=short",
          "-x",
          "-s",
          "-m", "builder",
          "--pyargs", "cf_test",
      ])
      if exit_code != 0:
          raise SystemExit(exit_code)
    '';
  }
