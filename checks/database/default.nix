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
  # cfFlakePath = pkgs.runCommand "cf-flake" {src = ../../.;} ''
  #   mkdir -p $out
  #   cp -r $src/* $out/
  # '';
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
    name = "crystal-forge-agent-integration";
    # Silence flake8/mypy for untyped helper lib
    skipLint = true;
    skipTypeCheck = true;
    nodes = {
      server = lib.crystal-forge.makeServerNode {
        inherit pkgs systemBuildClosure keyPath pubPath;
        extraConfig = {
          imports = [inputs.self.nixosModules.crystal-forge];
        };
        port = CF_TEST_SERVER_PORT;
      };
    };

    globalTimeout = 120; # Increased timeout for flake operations
    extraPythonPackages = p: [
      p.pytest
      p.pytest-xdist
      p.pytest-metadata
      p.pytest-html
      p.psycopg2
      pkgs.crystal-forge.cf-test-modules
      pkgs.crystal-forge.vm-test-logger
    ];

    testScript = ''
      import os
      import sys

      # Import cf_test and inject VM machines for pytest
      import cf_test
      cf_test._driver_machines = {"server": server}

      # Set up environment for cf_test
      os.environ.update({
          "NIXOS_TEST_DRIVER": "1",
          "CF_TEST_DB_HOST": "127.0.0.1",
          "CF_TEST_DB_PORT": "5432",  # Use the actual Crystal Forge DB port
          "CF_TEST_DB_NAME": "crystal_forge",
          "CF_TEST_DB_USER": "crystal_forge",
          "CF_TEST_DB_PASSWORD": "crystal_forge_password",
          "CF_TEST_SERVER_HOST": "127.0.0.1",
          "CF_TEST_SERVER_PORT": str(${toString CF_TEST_SERVER_PORT}),
      })

      # Wait for services to be ready
      print("🔄 Waiting for PostgreSQL service...")
      server.wait_for_unit("postgresql.service")

      # Wait a bit more for the database to be fully ready
      print("🔄 Checking database connectivity...")
      server.succeed("timeout 30 sh -c 'until pg_isready -h 127.0.0.1 -p 5432; do sleep 1; done'")

      print("🔄 Waiting for Crystal Forge server service...")
      server.wait_for_unit("crystal-forge-server.service")

      print("🔄 Waiting for server to be listening...")
      server.succeed("timeout 30 sh -c 'until ss -ltn | grep :3000; do sleep 1; done'")

      print("✅ All services ready!")

      # Run cf_test
      print("🧪 Running cf_test pytest suite...")
      import pytest
      exit_code = pytest.main([
          "-v",
          "--tb=short",
          "-m", "smoke or database",
          f"{cf_test.__path__[0]}"
      ])

      if exit_code == 0:
          print("✅ All tests passed!")
      else:
          print(f"❌ Tests failed with exit code {exit_code}")
          sys.exit(exit_code)
    '';
  }
