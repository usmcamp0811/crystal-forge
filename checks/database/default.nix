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
        port = 3000;
      };
    };

    globalTimeout = 900; # Increased timeout for flake operations
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
      import os, pytest, shutil

      from vm_test_logger import TestLogger
      start_all()

      logger = TestLogger("Crystal Forge Pytest (driver orchestrated)", server)
      logger.setup_logging()
      logger.gather_system_info(server)

      # Ensure services up in VM
      server.wait_for_unit("postgresql.service")
      server.wait_for_unit("crystal-forge-server.service")
      server.wait_for_open_port(3042)
      server.wait_for_open_port(3445)

      # Forward VM ports -> driver so pytest (running here) hits DB/API via localhost
      db_port  = server.forward_port(3042)
      api_port = server.forward_port(3445)

      os.environ["NIXOS_TEST_DRIVER"] = "1"
      os.environ["CF_TEST_DB_HOST"]     = "127.0.0.1"
      os.environ["CF_TEST_DB_PORT"]     = str(db_port)
      os.environ["CF_TEST_DB_NAME"]     = "crystal_forge"
      os.environ["CF_TEST_DB_USER"]     = "crystal_forge"
      os.environ["CF_TEST_DB_PASSWORD"] = "password"
      os.environ["CF_TEST_SERVER_HOST"] = "127.0.0.1"
      os.environ["CF_TEST_SERVER_PORT"] = str(api_port)

      # Inject Machine so tests can use the `machine` fixture
      import cf_test   # provided by pkgs.crystal-forge.cf-test-modules
      cf_test._driver_machine = server

      # Run your installed pytest suite from the driver
      os.makedirs("/tmp/cf-test-outputs", exist_ok=True)
      exit_code = pytest.main([
        "-vvv",
        "--maxfail=1",
        "--junitxml=/tmp/cf-test-outputs/junit.xml",
        "--pyargs", "cf_test",   # discover tests from your package
      ])

      # Always grab VM logs
      server.succeed('''
        set -eu
        mkdir -p /tmp/cf-test-outputs
        journalctl -u crystal-forge-server -u postgresql --no-pager > /tmp/cf-test-outputs/services.log || true
        systemctl status crystal-forge-server > /tmp/cf-test-outputs/server.status || true
        tar -C /tmp -czf /tmp/cf-test-outputs.tar.gz cf-test-outputs
      ''')

      # Copy artifacts VM -> host (becomes test result outputs)
      server.copy_from_vm("/tmp/cf-test-outputs/junit.xml", "junit.xml")
      server.copy_from_vm("/tmp/cf-test-outputs/services.log", "services.log")
      server.copy_from_vm("/tmp/cf-test-outputs/server.status", "server.status")
      server.copy_from_vm("/tmp/cf-test-outputs.tar.gz", "cf-test-outputs.tar.gz")

      logger.finalize_test()
      assert exit_code == 0, f"pytest failed with exit code {exit_code}"
    '';
  }
