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
          CrystalForgeServerTests,
          AgentTests,
          FlakeProcessingTests,
          SystemStateTests,
          ServiceLogCollector,
          DatabaseAnalyzer
      )
      from cf_test_modules.test_exceptions import AssertionFailedException  # type: ignore[import-untyped]

      def run_crystal_forge_integration_test():
          logger = TestLogger("Crystal Forge Agent Integration with Git Server", server)
          start_all()
          logger.setup_logging()
          system_info = logger.gather_system_info(agent)

          exit_on_failure = True
          ctx = CrystalForgeTestContext(
              gitserver=gitserver,
              server=server,
              agent=agent,
              logger=logger,
              system_info=system_info,
              exit_on_failure=exit_on_failure,
          )

          def run_test_phase(phase_name: str, test_func, *args, **kwargs):
              logger.log_section(f"🚀 STARTING: {phase_name}")
              try:
                  test_func(*args, **kwargs)
                  logger.log_success(f"✅ COMPLETED: {phase_name}")
              except AssertionFailedException as e:
                  logger.log_error(f"❌ ASSERTION FAILED: {phase_name}")
                  logger.log_error(f"🎯 Test: {e.test_name}")
                  logger.log_error(f"📝 Reason: {e.reason}")
                  if e.sql_query:
                      logger.log_error(f"🗄️ SQL involved: {e.sql_query[:200]}...")
                  raise
              except Exception as e:
                  import traceback, sys
                  tb = traceback.extract_tb(e.__traceback__)
                  if tb:
                      last_frame = tb[-1]
                      error_location = f"{last_frame.filename.split('/')[-1]}::{last_frame.name}() line {last_frame.lineno}"
                  else:
                      error_location = "unknown location"
                  logger.log_error(f"❌ FAILED: {phase_name}")
                  logger.log_error(f"📍 Error Location: {error_location}")
                  logger.log_error(f"📝 Error Message: {str(e)}")
                  logger.log_error(f"🔍 Error Type: {type(e).__name__}")
                  print(f"\n" + "="*80, file=sys.stderr)
                  print(f"❌ TEST PHASE FAILED: {phase_name}", file=sys.stderr)
                  print(f"Location: {error_location}", file=sys.stderr)
                  print(f"Error: {str(e)}", file=sys.stderr)
                  print(f"Type: {type(e).__name__}", file=sys.stderr)
                  print("="*80, file=sys.stderr)
                  raise

          try:
              run_test_phase("Phase 1.1: Git Server Setup", GitServerTests.setup_and_verify, ctx)
              run_test_phase("Phase 2.1: Crystal Forge Server Tests", CrystalForgeServerTests.setup_and_verify, ctx)
              run_test_phase("Phase 2.2: Agent Tests", AgentTests.setup_and_verify, ctx)
              run_test_phase("Phase 3.1: Flake Processing Tests", FlakeProcessingTests.verify_complete_workflow, ctx)
              run_test_phase("Phase 3.2: System State Tests", SystemStateTests.verify_system_state_tracking, ctx)

              try:
                  original_exit_setting = ctx.exit_on_failure
                  ctx.exit_on_failure = False
                  run_test_phase("Phase 4.1: Service Log Collection", ServiceLogCollector.collect_all_logs, ctx)
                  run_test_phase("Phase 4.2: Database Analysis", DatabaseAnalyzer.generate_comprehensive_report, ctx)
                  ctx.exit_on_failure = original_exit_setting
              except Exception as e:
                  logger.log_warning(f"⚠️ Analysis phases failed but continuing: {e}")

              logger.log_success("🎉 All Crystal Forge integration tests passed!")

          except AssertionFailedException as e:
              logger.log_error(f"💥 TEST ASSERTION FAILURE")
              logger.log_error(f"Test: {e.test_name}")
              logger.log_error(f"Reason: {e.reason}")
              try:
                  ServiceLogCollector.collect_all_logs(ctx)
              except:
                  pass
              raise
          except Exception as e:
              logger.log_error(f"💥 CRITICAL TEST FAILURE - SEE ERROR DETAILS ABOVE")
              try:
                  ServiceLogCollector.collect_all_logs(ctx)
              except:
                  pass
              raise
          finally:
              logger.finalize_test()

      run_crystal_forge_integration_test()
    '';
  }
