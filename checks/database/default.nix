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

    # testScript = ''
    #   import os
    #   from vm_test_logger import with_logging, TestPatterns
    #
    #   SERVER_PORT = ${toString CF_TEST_SERVER_PORT}
    #   DB_PORT = ${toString CF_TEST_DB_PORT}
    #
    #   @with_logging("Crystal Forge Agent Integration", primary_vm_name="server")
    #   def run(logger):
    #       start_all()
    #
    #       # Basic system info + wait for core services
    #       logger.gather_system_info(server)
    #       TestPatterns.standard_service_startup(
    #           logger, server,
    #           ["postgresql.service", "crystal-forge-server.service"]
    #       )
    #
    #       # Verify ports & binaries
    #       # TestPatterns.network_test(logger, server, "127.0.0.1", SERVER_PORT)
    #       logger.capture_command_output(
    #           server,
    #           "command -v run-cf-tests || which run-cf-tests",
    #           "which-run-cf-tests.txt",
    #           "Verify run-cf-tests is present",
    #       )
    #
    #       # Extra diagnostics before tests
    #       logger.capture_command_output(
    #           server,
    #           "ss -ltn",
    #           "listening-ports.txt",
    #           "Listening TCP ports",
    #       )
    #       logger.capture_command_output(
    #           server,
    #           "systemctl --no-pager --full status crystal-forge-server.service || true",
    #           "crystal-forge-server-status.txt",
    #           "crystal-forge-server status",
    #       )
    #
    #       # Run your pytest suite inside the server VM and tee output into /tmp/xchg
    #       logger.log_section("🏃 Running tests...")
    #
    #       # ✅ needs a separator after `pipefail`; also capture stderr
    #       server.succeed(
    #           "bash -lc 'set -euo pipefail; cf-test-runner -m vm_internal -vvv 2>&1 | tee /tmp/xchg/pytest-output.txt'"
    #       )
    #       logger.log_files.append("pytest-output.txt")
    #
    #       # Try to collect any pytest HTML reports if generated
    #       server.succeed(
    #           "bash -lc 'mkdir -p /tmp/xchg && "
    #           "find / -maxdepth 4 -type f -name \"report*.html\" -exec cp -t /tmp/xchg {} + 2>/dev/null || true'"
    #       )
    #       # Best-effort add common report names
    #       for name in ["report.html", "pytest-report.html"]:
    #           logger.log_files.append(name)
    #
    #       # Capture useful service logs (safe if absent)
    #       logger.capture_service_logs(server, "crystal-forge-server.service")
    #       logger.capture_service_logs(server, "postgresql.service")
    #
    #   run()
    # '';

    testScript = ''
      import os
      import pytest
      # --- Boot VMs first; otherwise any wait_* will hang
      os.environ["NIXOS_TEST_DRIVER"] = "1"
      start_all()

      server.wait_for_unit("postgresql.service")
      server.wait_for_open_port(5432)
      server.forward_port(5433, 5432)

      # Set environment variables for the test
      os.environ["CF_TEST_DB_HOST"] = "127.0.0.1"
      os.environ["CF_TEST_DB_PORT"] = "5433"  # forwarded port
      os.environ["CF_TEST_DB_USER"] = "postgres"
      os.environ["CF_TEST_DB_PASSWORD"] = ""  # no password for VM postgres
      os.environ["CF_TEST_SERVER_HOST"] = "127.0.0.1"
      os.environ["CF_TEST_SERVER_PORT"] = "${toString CF_TEST_SERVER_PORT}"
      # Inject machines so cf_test fixtures can drive them
      import cf_test
      cf_test._driver_machines = {
          "server": server,
      }
      # Run only VM-marked tests from cf_test package
      exit_code = pytest.main([
          "-vvvv",
          "--tb=short",
          "-x",
          "-s",  # Add -s to see print output immediately
          "-m", "database",
          "--pyargs", "cf_test",
      ])
      if exit_code != 0:
          raise SystemExit(exit_code)
    '';
  }
