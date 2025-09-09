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
    name = "crystal-forge-cache-integration";
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
        inherit pkgs systemBuildClosure keyPath pubPath;
        extraConfig = {
          imports = [inputs.self.nixosModules.crystal-forge];
          services.crystal-forge = {
            enable = true;
            local-database = true;
            server.enable = true;
            build.enable = true;
            database = {
              host = "localhost";
              port = 5432;
            };
            cache = {
              cache_type = "S3";
              push_to = "s3://crystal-forge-cache";
              push_after_build = true;
              s3_region = "us-east-1";
              parallel_uploads = 2;
              max_retries = 2;
              retry_delay_seconds = 1;
            };
            build.systemd_properties = [
              "Environment=AWS_ENDPOINT_URL=http://s3Cache:9000"
              "Environment=AWS_ACCESS_KEY_ID=minioadmin"
              "Environment=AWS_SECRET_ACCESS_KEY=minioadmin"
            ];
          };
        };
        port = CF_TEST_SERVER_PORT;
      };

      atticCache = lib.crystal-forge.makeAtticCacheNode {
        inherit pkgs;
        port = 8080;
      };

      atticServer = lib.crystal-forge.makeServerNode {
        inherit pkgs systemBuildClosure keyPath pubPath;
        extraConfig = {
          imports = [inputs.self.nixosModules.crystal-forge];
          services.crystal-forge = {
            enable = true;
            local-database = true;
            server.enable = true;
            build.enable = true;
            database = {
              host = "localhost";
              port = 5432;
            };
            cache = {
              cache_type = "Attic";
              push_after_build = true;
              attic_cache_name = "test";
              attic_token = "dGVzdCBzZWNyZXQgZm9yIGF0dGljZA==";
              max_retries = 2;
              retry_delay_seconds = 1;
            };
            build.systemd_properties = [
              "Environment=ATTIC_SERVER_URL=http://atticCache:8080"
            ];
          };
        };
        port = CF_TEST_SERVER_PORT;
      };
    };

    globalTimeout = 1200; # 20 minutes for cache operations
    extraPythonPackages = p: [p.pytest pkgs.crystal-forge.vm-test-logger pkgs.crystal-forge.cf-test-modules];

    testScript = ''
      import os
      import pytest

      os.environ["NIXOS_TEST_DRIVER"] = "1"
      start_all()

      # Wait for cache services
      s3Cache.wait_for_unit("minio.service")
      s3Cache.wait_for_unit("minio-setup.service")
      s3Cache.wait_for_open_port(9000)

      atticCache.wait_for_unit("atticd.service")
      atticCache.wait_for_unit("attic-setup.service")
      atticCache.wait_for_open_port(8080)


      # Wait for servers
      s3Server.wait_for_unit("postgresql.service")
      s3Server.wait_for_unit("crystal-forge-server.service")
      s3Server.wait_for_open_port(5432)
      s3Server.forward_port(5433, 5432)

      atticServer.wait_for_unit("postgresql.service")
      atticServer.wait_for_unit("crystal-forge-server.service")
      atticServer.wait_for_open_port(5432)
      atticServer.forward_port(5434, 5432)

      from cf_test.vm_helpers import wait_for_git_server_ready
      wait_for_git_server_ready(gitserver, timeout=120)

      # Set up test environment variables
      os.environ["CF_TEST_GIT_SERVER_URL"] = "http://gitserver/crystal-forge"
      os.environ["CF_TEST_REAL_COMMIT_HASH"] = "${testFlakeCommitHash}"

      # S3 test environment
      os.environ["CF_TEST_S3_DB_HOST"] = "127.0.0.1"
      os.environ["CF_TEST_S3_DB_PORT"] = "5433"
      os.environ["CF_TEST_S3_SERVER_HOST"] = "127.0.0.1"
      os.environ["CF_TEST_S3_SERVER_PORT"] = "${toString CF_TEST_SERVER_PORT}"

      # Attic test environment
      os.environ["CF_TEST_ATTIC_DB_HOST"] = "127.0.0.1"
      os.environ["CF_TEST_ATTIC_DB_PORT"] = "5434"
      os.environ["CF_TEST_ATTIC_SERVER_HOST"] = "127.0.0.1"
      os.environ["CF_TEST_ATTIC_SERVER_PORT"] = "${toString CF_TEST_SERVER_PORT}"

      # Inject machines for test access
      import cf_test
      cf_test._driver_machines = {
          "s3Server": s3Server,
          "s3Cache": s3Cache,
          "atticServer": atticServer,
          "atticCache": atticCache,
          "gitserver": gitserver,
      }

      # Run cache-specific tests
      exit_code = pytest.main([
          "-vvvv",
          "--tb=short",
          "-x",
          "-s",
          "-m", "cache",
          "--pyargs", "cf_test",
      ])
      if exit_code != 0:
          raise SystemExit(exit_code)
    '';
  }
