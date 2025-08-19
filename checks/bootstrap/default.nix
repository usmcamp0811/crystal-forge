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
    extraPythonPackages = p: [p.pytest pkgs.crystal-forge.cf-test-modules];

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

          try:
              # Phase 1: Infrastructure Setup
              GitServerTests.setup_and_verify(ctx)
              DatabaseTests.setup_and_verify(ctx)

              # Phase 2: Service Startup and Verification
              CrystalForgeServerTests.setup_and_verify(ctx)
              AgentTests.setup_and_verify(ctx)

              # Phase 3: Core Workflow Testing
              FlakeProcessingTests.verify_complete_workflow(ctx)
              SystemStateTests.verify_system_state_tracking(ctx)

              # Phase 4: Analysis and Artifact Collection
              ServiceLogCollector.collect_all_logs(ctx)
              DatabaseAnalyzer.generate_comprehensive_report(ctx)

              logger.log_success("🎉 All Crystal Forge integration tests passed!")

          except Exception as e:
              logger.log_error(f"Test failed: {str(e)}")
              ServiceLogCollector.collect_all_logs(ctx)
              raise
          finally:
              logger.finalize_test()

      # Execute the main test
      run_crystal_forge_integration_test()
    '';
  }
