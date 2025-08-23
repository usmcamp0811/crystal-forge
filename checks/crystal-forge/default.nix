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
          GitServerTests,
          DatabaseTests,
          CrystalForgeServerTests,
          AgentTests,
          FlakeProcessingTests,
          SystemStateTests,
          ServiceLogCollector,
          DatabaseAnalyzer
      )

      def run_crystal_forge_integration_test():
          """Main test orchestrator function"""

          # Initialize components
          logger = TestLogger("Crystal Forge Agent Integration with Git Server", server)
          start_all()
          logger.setup_logging()

          # Gather system information early
          system_info = logger.gather_system_info(agent)

          # Create test context
          ctx = CrystalForgeTestContext(
              gitserver=gitserver,
              server=server,
              agent=agent,
              logger=logger,
              system_info=system_info
          )

          def run_test_phase(phase_name: str, test_func, *args, **kwargs):
              """Helper to run a test phase with proper error handling"""
              logger.log_section(f"🚀 STARTING: {phase_name}")
              try:
                  test_func(*args, **kwargs)
                  logger.log_success(f"✅ COMPLETED: {phase_name}")
              except Exception as e:
                  import traceback
                  import sys

                  # Get more detailed error info
                  tb = traceback.extract_tb(e.__traceback__)
                  if tb:
                      last_frame = tb[-1]
                      error_location = f"{last_frame.filename.split('/')[-1]}::{last_frame.name}() line {last_frame.lineno}"
                  else:
                      error_location = "unknown location"

                  logger.log_error(f"❌ FAILED: {phase_name}")
                  logger.log_error(f"🔍 Error Location: {error_location}")
                  logger.log_error(f"🔍 Error Message: {str(e)}")
                  logger.log_error(f"🔍 Error Type: {type(e).__name__}")

                  # Print to stderr for immediate visibility
                  print(f"\n" + "="*80, file=sys.stderr)
                  print(f"❌ TEST PHASE FAILED: {phase_name}", file=sys.stderr)
                  print(f"Location: {error_location}", file=sys.stderr)
                  print(f"Error: {str(e)}", file=sys.stderr)
                  print(f"Type: {type(e).__name__}", file=sys.stderr)
                  print("="*80, file=sys.stderr)

                  raise

          try:
              # Phase 1: Infrastructure Setup
              run_test_phase("Phase 1.1: Git Server Setup", GitServerTests.setup_and_verify, ctx)
              run_test_phase("Phase 1.2: Database Setup", DatabaseTests.setup_and_verify, ctx)

              # Phase 2: Service Startup and Verification
              run_test_phase("Phase 2.1: Crystal Forge Server Tests", CrystalForgeServerTests.setup_and_verify, ctx)

              # Phase 2.1b: Database View Tests (NOW that server is running)
              run_test_phase("Phase 2.1b: Database View Tests", DatabaseTests.run_view_tests, ctx)

              run_test_phase("Phase 2.2: Agent Tests", AgentTests.setup_and_verify, ctx)

              # Phase 3: Core Workflow Testing
              run_test_phase("Phase 3.1: Flake Processing Tests", FlakeProcessingTests.verify_complete_workflow, ctx)
              run_test_phase("Phase 3.2: System State Tests", SystemStateTests.verify_system_state_tracking, ctx)

              # Phase 4: Analysis and Artifact Collection (non-critical)
              try:
                  run_test_phase("Phase 4.1: Service Log Collection", ServiceLogCollector.collect_all_logs, ctx)
              except Exception as e:
                  logger.log_warning(f"⚠️ Phase 4.1 failed but continuing: {e}")

              try:
                  run_test_phase("Phase 4.2: Database Analysis", DatabaseAnalyzer.generate_comprehensive_report, ctx)
              except Exception as e:
                  logger.log_warning(f"⚠️ Phase 4.2 failed but continuing: {e}")

              logger.log_success("🎉 All Crystal Forge integration tests passed!")

          except Exception as e:
              logger.log_error(f"💥 CRITICAL TEST FAILURE - SEE ERROR DETAILS ABOVE")
              # Always try to collect logs on failure
              try:
                  ServiceLogCollector.collect_all_logs(ctx)
              except:
                  pass  # Don't let log collection failure mask the real error
              raise
          finally:
              logger.finalize_test()

      # Execute the main test
      run_crystal_forge_integration_test()
    '';
  }
