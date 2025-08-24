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

      # your logger package
      from vm_test_logger import TestLogger

      def dump_quick(tag: str):
          server.log(f"=== {tag}: quick dump ===")
          server.succeed("journalctl -u postgresql -n 200 --no-pager || true")
          server.succeed("journalctl -u crystal-forge-server -n 200 --no-pager || true")
          server.succeed("ss -ltnp || true")

      start_all()

      # logger in VM (writes /tmp/xchg/* and copies to host on finalize)
      logger = TestLogger("Crystal Forge DB/Server Pytest", server)
      logger.setup_logging()
      sysinfo = logger.gather_system_info(server)

      # wait + quick dumps on failure
      try:
        logger.wait_for_services(server, ["postgresql.service", "crystal-forge-server.service"])
        server.wait_for_open_port(3042)
        server.wait_for_open_port(3445)
      except Exception:
        dump_quick("service-wait-failed")
        raise

      # forward ports to the driver; run pytest in driver; inject Machine for tests
      db_port  = server.forward_port(3042)
      api_port = server.forward_port(3445)

      os.environ.update({
        "NIXOS_TEST_DRIVER": "1",
        "CF_TEST_DB_HOST": "127.0.0.1",
        "CF_TEST_DB_PORT": str(db_port),
        "CF_TEST_DB_NAME": "crystal_forge",
        "CF_TEST_DB_USER": "crystal_forge",
        "CF_TEST_DB_PASSWORD": "password",
        "CF_TEST_SERVER_HOST": "127.0.0.1",
        "CF_TEST_SERVER_PORT": str(api_port),
      })

      import cf_test  # your pytest package
      cf_test._driver_machine = server  # enables `machine` fixture in tests

      os.makedirs("/tmp/cf-test-outputs", exist_ok=True)
      exit_code = 1
      try:
        exit_code = pytest.main([
          "-vvv",
          "--maxfail=1",
          "--junitxml=/tmp/cf-test-outputs/junit.xml",
          "--pyargs", "cf_test",
        ])
      finally:
        # capture useful logs with your logger helpers
        logger.capture_service_logs(server, "postgresql.service", "postgres.log")
        logger.capture_service_logs(server, "crystal-forge-server.service", "server.log")
        logger.capture_command_output(server, "systemctl --no-pager --full status crystal-forge-server", "server.status", "crystal-forge-server status")
        logger.capture_command_output(server, "systemctl --no-pager --full status postgresql", "postgres.status", "postgresql status")

        # copy pytest artifacts VM -> host
        try:
          server.copy_from_vm("/tmp/cf-test-outputs/junit.xml", "junit.xml")
        except Exception:
          pass

        # finalize (also copies /tmp/xchg/* via logger.copy_logs_from_vm)
        logger.finalize_test()

      assert exit_code == 0, f"pytest failed with exit code {exit_code}"
    '';
  }
