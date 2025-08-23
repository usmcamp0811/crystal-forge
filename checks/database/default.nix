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
  cfFlakePath = pkgs.runCommand "cf-flake" {src = ../../.;} ''
    mkdir -p $out
    cp -r $src/* $out/
  '';
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
      gitserver = lib.crystal-forge.makeGitServerNode {
        inherit pkgs systemBuildClosure;
        port = 8080;
      };

      server = lib.crystal-forge.makeServerNode {
        inherit pkgs systemBuildClosure keyPath pubPath cfFlakePath;
        extraConfig = {
          imports = [inputs.self.nixosModules.crystal-forge];
        };
        port = 3000;
      };

      agent = lib.crystal-forge.makeAgentNode {
        inherit pkgs systemBuildClosure inputs keyPath pubPath;
        serverHost = "server";
        extraConfig = {imports = [inputs.self.nixosModules.crystal-forge];};
      };
    };

    globalTimeout = 900; # Increased timeout for flake operations
    extraPythonPackages = p: [p.pytest pkgs.crystal-forge.vm-test-logger pkgs.crystal-forge.cf-test-modules];

    testScript = ''
      from vm_test_logger import TestLogger  # type: ignore[import-untyped]
      from cf_test_modules import (  # type: ignore[import-untyped]
          CrystalForgeTestContext,
          DatabaseTests,
      )
      from cf_test_modules.test_exceptions import AssertionFailedException  # type: ignore[import-untyped]

      def run_database_tests():
          # Boot VMs and set up logging
          logger = TestLogger("Crystal Forge Database Tests", server)
          start_all()
          logger.setup_logging()
          system_info = logger.gather_system_info(agent)

          # Only run DB-related tests
          ctx = CrystalForgeTestContext(
              gitserver=gitserver,
              server=server,
              agent=agent,
              logger=logger,
              system_info=system_info,
              exit_on_failure=True,
          )

          def run_phase(name, func, *args, **kwargs):
              logger.log_section(f"🚀 STARTING: {name}")
              try:
                  func(*args, **kwargs)
                  logger.log_success(f"✅ COMPLETED: {name}")
              except AssertionFailedException as e:
                  logger.log_error(f"❌ ASSERTION FAILED: {name}")
                  logger.log_error(f"📝 {e}")
                  raise
              except Exception as e:
                  import traceback
                  tb = traceback.extract_tb(e.__traceback__)
                  if tb:
                      last = tb[-1]
                      loc = f"{last.filename.split('/')[-1]}::{last.name}() line {last.lineno}"
                  else:
                      loc = "unknown"
                  logger.log_error(f"❌ FAILED: {name}")
                  logger.log_error(f"📍 {loc}")
                  logger.log_error(f"📝 {e}")
                  raise

          try:
              run_phase("Phase DB.1: Database Setup", DatabaseTests.setup_and_verify, ctx)
              run_phase("Phase DB.2: Database View Tests", DatabaseTests.run_view_tests, ctx)
              logger.log_success("🎉 Database-related tests passed!")
          finally:
              logger.finalize_test()

      run_database_tests()
    '';
  }
