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

      s3Server = lib.crystal-forge.mkServerNode {
        inherit inputs pkgs systemBuildClosure keyPath pubPath;
        port = CF_TEST_SERVER_PORT;

        # Only specify what you want to change/add for this specific test
        crystalForgeConfig = {
          # Add S3 cache configuration
          cache = {
            cache_type = "S3";
            push_to = "s3://crystal-forge-cache";
            push_after_build = true;
            s3_region = "us-east-1";
            parallel_uploads = 2;
            max_retries = 2;
            retry_delay_seconds = 1;
          };

          # Override build config to add S3 environment variables
          build = {
            enable = true;
            offline = true; # This will merge with the default
            systemd_properties = [
              "Environment=AWS_ENDPOINT_URL=http://s3Cache:9000"
              "Environment=AWS_ACCESS_KEY_ID=minioadmin"
              "Environment=AWS_SECRET_ACCESS_KEY=minioadmin"
            ];
          };

          # Override just the flakes.watched - will completely replace the default
          flakes.watched = [
            {
              name = "test-flake";
              repo_url = "http://gitserver/crystal-forge";
              auto_poll = true;
            }
          ];

          # Override environments - will completely replace the default
          environments = [
            {
              name = "test";
              description = "Computers that get on wifi";
              is_active = true;
              risk_profile = "MEDIUM"; # Different from default LOW
              compliance_level = "NONE";
            }
          ];

          # Override server config - will merge with defaults
          server = {
            port = CF_TEST_SERVER_PORT; # Override the port
            # enable and host will use defaults
          };

          # Override database config if needed
          database = {
            host = "localhost";
            user = "crystal_forge";
            name = "crystal_forge";
            port = 5432;
          };
        };

        # General NixOS configuration (non-crystal-forge stuff)
        extraConfig = lib.crystal-forge.preloadTestFlake {
          commitNumber = 5;
          branch = "main";
        };
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

      # Read commit hashes directly from testFlake metadata files
      main_head = "${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/MAIN_HEAD"))}"
      dev_head = "${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/DEVELOPMENT_HEAD"))}"
      feature_head = "${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/FEATURE_HEAD"))}"

      # Multi-branch commit hashes (read from testFlake metadata)
      main_commits = """${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/MAIN_COMMITS"))}"""
      dev_commits = """${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/DEVELOPMENT_COMMITS"))}"""
      feature_commits = """${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/FEATURE_COMMITS"))}"""

      # Set up test environment variables for multi-branch test flake
      os.environ["CF_TEST_GIT_SERVER_URL"] = "http://gitserver/crystal-forge"
      os.environ["CF_TEST_REAL_REPO_URL"] = "http://gitserver/crystal-forge"

      # Use main branch head as the primary test commit
      os.environ["CF_TEST_REAL_COMMIT_HASH"] = main_head

      # Branch head commits
      os.environ["CF_TEST_MAIN_HEAD"] = main_head
      os.environ["CF_TEST_DEVELOPMENT_HEAD"] = dev_head
      os.environ["CF_TEST_FEATURE_HEAD"] = feature_head

      # Commit lists (convert newlines to commas for easier parsing)
      os.environ["CF_TEST_MAIN_COMMITS"] = main_commits.replace('\n', ',')
      os.environ["CF_TEST_DEVELOPMENT_COMMITS"] = dev_commits.replace('\n', ',')
      os.environ["CF_TEST_FEATURE_COMMITS"] = feature_commits.replace('\n', ',')

      # Commit counts
      os.environ["CF_TEST_MAIN_COMMIT_COUNT"] = "${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/MAIN_COMMIT_COUNT"))}"
      os.environ["CF_TEST_DEVELOPMENT_COMMIT_COUNT"] = "${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/DEVELOPMENT_COMMIT_COUNT"))}"
      os.environ["CF_TEST_FEATURE_COMMIT_COUNT"] = "${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/FEATURE_COMMIT_COUNT"))}"

      # Database connection info
      os.environ["CF_TEST_DB_HOST"] = "127.0.0.1"
      os.environ["CF_TEST_DB_PORT"] = "5433"
      os.environ["CF_TEST_DB_USER"] = "postgres"
      os.environ["CF_TEST_DB_PASSWORD"] = ""  # no password for VM postgres

      # Server connection info
      os.environ["CF_TEST_SERVER_HOST"] = "127.0.0.1"
      os.environ["CF_TEST_SERVER_PORT"] = "${toString CF_TEST_SERVER_PORT}"

      # Derivation paths JSON
      os.environ["CF_TEST_DRV"] = "${derivation-paths}"

      # Flake information for tests
      os.environ["CF_TEST_FLAKE_NAME"] = "test-flake"
      os.environ["CF_TEST_PRELOADED_FLAKE_PATH"] = "/etc/preloaded-flake"

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
          "-m", "s3cache",
          "--pyargs", "cf_test",
      ])
      if exit_code != 0:
          raise SystemExit(exit_code)
    '';
  }
