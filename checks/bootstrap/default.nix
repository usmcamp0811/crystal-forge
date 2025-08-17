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
    extraPythonPackages = p: [p.pytest];
    testScript = ''
      import pytest
      import time

      def log_to_file(message):
          """Log message to both console and file"""
          print(message)
          try:
              with open("/tmp/xchg/test-results.log", "a") as f:
                  f.write(f"{message}\n")
          except FileNotFoundError:
              # File will be created after VMs start
              pass

      log_to_file("🚀 Starting Crystal Forge Agent Integration Test")
      log_to_file("=" * 60)
      log_to_file(f"Test started at: {time.strftime('%Y-%m-%d %H:%M:%S UTC', time.gmtime())}")

      start_all()

      # Initialize log directories after VMs start
      server.succeed("mkdir -p /tmp/xchg")
      agent.succeed("mkdir -p /tmp/xchg")

      # Re-initialize the log file with proper header after VMs are ready
      server.succeed('echo "🚀 Starting Crystal Forge Agent Integration Test" > /tmp/xchg/test-results.log')
      server.succeed('echo "=" >> /tmp/xchg/test-results.log')
      server.succeed(f'echo "Test started at: {time.strftime("%Y-%m-%d %H:%M:%S UTC", time.gmtime())}" >> /tmp/xchg/test-results.log')
      server.succeed('echo "✅ All VMs started successfully" >> /tmp/xchg/test-results.log')

      def log_to_file_vm(message):
          """Log message to both console and VM file"""
          print(message)
          # Escape single quotes for shell
          escaped_msg = message.replace("'", "'\"'\"'")
          server.succeed(f"echo '{escaped_msg}' >> /tmp/xchg/test-results.log")

      # Check initial service status
      log_to_file_vm("\n📊 Checking initial service status...")
      server.succeed("systemctl status crystal-forge-server.service || true")

      log_to_file_vm("\n🔍 Capturing Crystal Forge server logs...")
      server.log("=== crystal-forge-server service logs ===")
      server_logs = server.succeed("journalctl -u crystal-forge-server.service --no-pager || true")

      # Save server logs to file
      server.succeed("echo 'Crystal Forge Server Logs:' > /tmp/xchg/server-logs.txt")
      server.succeed("journalctl -u crystal-forge-server.service --no-pager >> /tmp/xchg/server-logs.txt || true")

      # Wait for essential services
      log_to_file_vm("\n⏳ Waiting for essential services to start...")
      log_to_file_vm("  • PostgreSQL database...")
      server.wait_for_unit("postgresql")
      log_to_file_vm("  ✅ PostgreSQL is ready")

      log_to_file_vm("  • Crystal Forge server...")
      server.wait_for_unit("crystal-forge-server.service")
      log_to_file_vm("  ✅ Crystal Forge server is ready")

      log_to_file_vm("  • Crystal Forge agent...")
      agent.wait_for_unit("crystal-forge-agent.service")
      log_to_file_vm("  ✅ Crystal Forge agent is ready")

      log_to_file_vm("  • Multi-user target...")
      server.wait_for_unit("multi-user.target")
      log_to_file_vm("  ✅ All services operational")

      # Verify key files
      log_to_file_vm("\n🔑 Verifying key file accessibility...")
      agent.succeed("test -r /etc/agent.key")
      log_to_file_vm("  ✅ Agent private key accessible")

      agent.succeed("test -r /etc/agent.pub")
      log_to_file_vm("  ✅ Agent public key accessible on agent")

      server.succeed("test -r /etc/agent.pub")
      log_to_file_vm("  ✅ Agent public key accessible on server")

      # Test network connectivity
      log_to_file_vm("\n🌐 Testing network connectivity...")
      port_check = server.succeed("ss -ltn | grep ':3000'")
      log_to_file_vm("  ✅ Server listening on port 3000")

      ping_result = agent.succeed("ping -c1 server")
      log_to_file_vm("  ✅ Agent can reach server")

      # Gather system information
      log_to_file_vm("\n📋 Gathering system information...")
      agent_hostname = agent.succeed("hostname -s").strip()
      system_hash = agent.succeed("readlink /run/current-system").strip().split("-")[-1]
      change_reason = "startup"

      log_to_file_vm(f"  • Agent hostname: {agent_hostname}")
      log_to_file_vm(f"  • System hash: {system_hash}")
      log_to_file_vm(f"  • Expected change reason: {change_reason}")

      # Capture agent logs
      agent.succeed("echo 'Crystal Forge Agent Logs:' > /tmp/xchg/agent-logs.txt")
      agent.succeed("journalctl -u crystal-forge-agent.service --no-pager >> /tmp/xchg/agent-logs.txt || true")

      # Wait for agent connection
      log_to_file_vm("\n🤝 Waiting for agent to connect to server...")
      server.wait_until_succeeds("journalctl -u crystal-forge-server.service | grep -E 'accepted.*agent'")
      log_to_file_vm("  ✅ Agent successfully connected to server")

      # Verify database entries
      log_to_file_vm("\n🗄️  Verifying database entries...")
      output = server.succeed("psql -U crystal_forge -d crystal_forge -c 'SELECT hostname, derivation_path, change_reason FROM system_states;'")

      log_to_file_vm("Database query result:")
      log_to_file_vm("-" * 40)
      # Write database output to log file (handle multiline)
      server.succeed("echo 'Database Query Results:' >> /tmp/xchg/test-results.log")
      server.succeed("psql -U crystal_forge -d crystal_forge -c 'SELECT hostname, derivation_path, change_reason FROM system_states;' >> /tmp/xchg/test-results.log")
      log_to_file_vm("-" * 40)

      # Save database dump
      server.succeed("echo 'Database Schema and Contents:' > /tmp/xchg/database-dump.txt")
      server.succeed("psql -U crystal_forge -d crystal_forge -c '\\dt' >> /tmp/xchg/database-dump.txt")
      server.succeed("echo >> /tmp/xchg/database-dump.txt")
      server.succeed("psql -U crystal_forge -d crystal_forge -c 'SELECT * FROM system_states;' >> /tmp/xchg/database-dump.txt")
      server.succeed("echo >> /tmp/xchg/database-dump.txt")
      server.succeed("psql -U crystal_forge -d crystal_forge -c 'SELECT * FROM environments;' >> /tmp/xchg/database-dump.txt")

      # Validate expected data
      log_to_file_vm("\n✅ Validating expected data in database...")

      if agent_hostname not in output:
          log_to_file_vm(f"❌ FAIL: hostname '{agent_hostname}' not found in database")
          pytest.fail(f"hostname '{agent_hostname}' not found in DB")
      else:
          log_to_file_vm(f"  ✅ Hostname '{agent_hostname}' found in database")

      if change_reason not in output:
          log_to_file_vm(f"❌ FAIL: change_reason '{change_reason}' not found in database")
          pytest.fail(f"change_reason '{change_reason}' not found in DB")
      else:
          log_to_file_vm(f"  ✅ Change reason '{change_reason}' found in database")

      if system_hash not in output:
          log_to_file_vm(f"❌ FAIL: system hash '{system_hash}' not found in database")
          pytest.fail(f"derivation_path '{system_hash}' not found in DB")
      else:
          log_to_file_vm(f"  ✅ System hash '{system_hash}' found in database")

      # Show additional system state
      log_to_file_vm("\n📊 Additional system information...")
      systems_count = server.succeed("psql -U crystal_forge -d crystal_forge -c 'SELECT COUNT(*) FROM system_states;' -t").strip()
      log_to_file_vm(f"  • Total system states recorded: {systems_count}")

      environments_count = server.succeed("psql -U crystal_forge -d crystal_forge -c 'SELECT COUNT(*) FROM environments;' -t").strip()
      log_to_file_vm(f"  • Total environments configured: {environments_count}")

      # Final status check
      log_to_file_vm(f"\n🎉 Test completed successfully at: {time.strftime('%Y-%m-%d %H:%M:%S UTC', time.gmtime())}")
      log_to_file_vm("=" * 60)
      log_to_file_vm("✅ All assertions passed")
      log_to_file_vm("✅ Agent-server communication verified")
      log_to_file_vm("✅ Database integration confirmed")
      log_to_file_vm("✅ Crystal Forge integration test PASSED")

      log_to_file_vm("\n📁 Log files will be copied to host:")
      log_to_file_vm("  • test-results.log - Main test execution log")
      log_to_file_vm("  • server-logs.txt - Crystal Forge server service logs")
      log_to_file_vm("  • agent-logs.txt - Crystal Forge agent service logs")
      log_to_file_vm("  • database-dump.txt - Database state and contents")

      # Copy all log files from VMs to host
      server.copy_from_vm("/tmp/xchg/test-results.log")
      server.copy_from_vm("/tmp/xchg/server-logs.txt")
      server.copy_from_vm("/tmp/xchg/database-dump.txt")
      agent.copy_from_vm("/tmp/xchg/agent-logs.txt")
    '';
  }
