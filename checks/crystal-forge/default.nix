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
  key = pkgs.runCommand "agent.key" {} ''
    mkdir -p $out
    cp ${keyPair}/agent.key $out/
  '';
  pub = pkgs.runCommand "agent.pub" {} ''
    mkdir -p $out
    cp ${keyPair}/agent.pub $out/
  '';
in
  pkgs.testers.runNixOSTest {
    name = "crystal-forge-agent-integration";

    nodes = {
      server = {
        config,
        pkgs,
        ...
      }: {
        imports = [inputs.self.nixosModules.crystal-forge];

        networking.useDHCP = true;
        networking.firewall.allowedTCPPorts = [3000];

        environment.etc."agent.key".source = "${key}/agent.key";
        environment.etc."agent.pub".source = "${pub}/agent.pub";

        services.postgresql = {
          enable = true;
          authentication = lib.concatStringsSep "\n" [
            "local all root trust"
            "local all postgres peer"
          ];
        };
        services.crystal-forge = {
          enable = true;
          local-database = true;
          database = {
            user = "crystal_forge";
            dbname = "crystal_forge";
          };
          server = {
            enable = true;
            host = "0.0.0.0";
            port = 3000;
            authorized_keys = {
              agent = builtins.readFile "${pub}/agent.pub";
            };
          };
        };
      };
      agent = {
        config,
        pkgs,
        ...
      }: {
        imports = [inputs.self.nixosModules.crystal-forge];

        networking.useDHCP = true;
        networking.firewall.enable = false;

        environment.etc."agent.key".source = "${key}/agent.key";
        environment.etc."agent.pub".source = "${pub}/agent.pub";

        services.crystal-forge = {
          enable = true;
          client = {
            enable = true;
            server_host = "server";
            server_port = 3000;
            private_key = "/etc/agent.key";
          };
        };
      };
    };

    testScript = ''
      start_all()

      server.wait_for_unit("postgresql")
      server.succeed("systemctl show -p Result crystal-forge-init-db.service | grep '=success'")
      server.wait_for_unit("crystal-forge-server.service")
      agent.wait_for_unit("crystal-forge-agent.service")
      agent.wait_for_file("/run/current-system")
      server.wait_for_unit("multi-user.target")

      agent_hostname = agent.succeed("hostname -s").strip()
      system_hash = agent.succeed("readlink /run/current-system").strip().split("-")[-1]
      context = "agent-startup"

      # Wait for server to log the submission
      server.wait_until_succeeds("journalctl -u crystal-forge-server.service | grep 'accepted from agent'")

      agent.log("=== agent logs ===")
      agent.log(agent.succeed("journalctl -u crystal-forge-agent.service || true"))

      # Now safe to query the DB
      output = server.succeed("psql -U crystal_forge -d crystal_forge -c 'SELECT hostname, system_derivation_id, context FROM system_state;'")
      server.log("Final DB state:\n" + output)

      # Ensure PostgreSQL is not running on the agent
      assert "postgresql" not in agent.succeed("systemctl list-units --type=service --state=active"), "PostgreSQL is unexpectedly running on the agent"

      assert agent_hostname in output, f"hostname '{agent_hostname}' not found in DB"
      assert context in output, f"context '{context}' not found in DB"
      assert system_hash in output, f"system_derivation_id '{system_hash}' not found in DB"
    '';
  }
