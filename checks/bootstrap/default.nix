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
    rootPaths = [
      inputs.self.nixosConfigurations.cf-test-sys.config.system.build.toplevel
      pkgs.crystal-forge.default
      pkgs.path
    ];
  };
in
  pkgs.testers.runNixOSTest {
    name = "crystal-forge-agent-integration";

    # Silence flake8/mypy for untyped helper lib
    skipLint = true;
    skipTypeCheck = true;

    nodes = {
      server = lib.crystal-forge.makeServerNode {
        inherit pkgs systemBuildClosure keyPath pubPath cfFlakePath;
        extraConfig = {imports = [inputs.self.nixosModules.crystal-forge];};
        port = 3000;
      };

      agent = lib.crystal-forge.makeAgentNode {
        inherit pkgs inputs keyPath pubPath;
        serverHost = "server";
        extraConfig = {imports = [inputs.self.nixosModules.crystal-forge];};
      };
    };

    globalTimeout = 600;
    extraPythonPackages = p: [p.pytest pkgs.crystal-forge.vm-test-logger];

    testScript = ''
      from vm_test_logger import TestLogger, TestPatterns  # type: ignore[import-untyped]

      logger = TestLogger("Crystal Forge Agent Integration", server)

      start_all()
      logger.setup_logging()

      TestPatterns.standard_service_startup(logger, server, [
        "postgresql",
        "crystal-forge-server.service",
        "multi-user.target",
      ])

      TestPatterns.standard_service_startup(logger, agent, [
        "crystal-forge-agent.service",
      ])

      # Capture service logs with better error handling
      TestPatterns.capture_service_logs_multi_vm(logger, [
        (server, "crystal-forge-server.service"),
        (agent, "crystal-forge-agent.service"),
      ])

      TestPatterns.key_file_verification(logger, agent, {
        "/etc/agent.key": "Agent private key accessible",
        "/etc/agent.pub": "Agent public key accessible on agent",
      })

      server.succeed("test -r /etc/agent.pub")
      logger.log_success("Agent public key accessible on server")

      TestPatterns.network_test(logger, server, "server", 3000)

      system_info = logger.gather_system_info(agent)

      logger.log_section("🤝 Waiting for agent to connect to server...")
      server.wait_until_succeeds("journalctl -u crystal-forge-server.service | grep -E 'accepted.*agent'")
      logger.log_success("Agent successfully connected to server")

      # TestPatterns.database_verification(logger, server, "crystal_forge", {
      #   "hostname": system_info['hostname'],
      #   "system_hash": system_info['system_hash'],
      #   "change_reason": "startup",
      # })

      systems_count = server.succeed(
        "psql -U crystal_forge -d crystal_forge -c 'SELECT COUNT(*) FROM system_states;' -t"
      ).strip()
      logger.log_info(f"Total system states recorded: {systems_count}")

      logger.finalize_test()
    '';
  }
