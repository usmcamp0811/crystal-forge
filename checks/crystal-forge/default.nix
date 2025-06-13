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
          flakes.watched = {
            dotfiles = "git+https://gitlab.com/usmcamp0811/dotfiles";
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
      server.wait_for_unit("crystal-forge-server.service")
      agent.wait_for_unit("crystal-forge-agent.service")
      server.wait_for_unit("multi-user.target")

      # Ensure keys are available
      agent.succeed("test -r /etc/agent.key")
      agent.succeed("test -r /etc/agent.pub")
      server.succeed("test -r /etc/agent.pub")

      # Confirm server is listening
      server.succeed("ss -ltn | grep ':3000'")

      # Confirm agent can ping server
      agent.succeed("ping -c1 server")

      agent_hostname = agent.succeed("hostname -s").strip()
      system_hash = agent.succeed("readlink /run/current-system").strip().split("-")[-1]
      context = "agent-startup"

      # Wait for initial system state to be recorded
      server.wait_until_succeeds("journalctl -u crystal-forge-server.service | grep 'accepted from agent'")

      # Agent logs
      agent.log("=== agent logs ===")
      agent.log(agent.succeed("journalctl -u crystal-forge-agent.service || true"))

      # System state should be written to DB
      output = server.succeed("psql -U crystal_forge -d crystal_forge -c 'SELECT hostname, system_derivation_id, context FROM tbl_system_states;'")
      server.log("Final DB state:\n" + output)

      assert agent_hostname in output, f"hostname '{agent_hostname}' not found in DB"
      assert context in output, f"context '{context}' not found in DB"
      assert system_hash in output, f"system_derivation_id '{system_hash}' not found in DB"

      # POST webhook to simulate external trigger
      commit_hash = "2abc071042b61202f824e7f50b655d00dfd07765"
      curl_data = f"""'{{
        "project": {{
          "web_url": "git+https://gitlab.com/usmcamp0811/dotfiles"
        }},
        "checkout_sha": "{commit_hash}"
      }}'"""

      server.succeed(f"curl -s -X POST http://localhost:3000/webhook -H 'Content-Type: application/json' -d {curl_data}")

      # Wait until commit is processed
      # server.wait_until_succeeds(f"journalctl -u crystal-forge-server.service | grep {commit_hash}")

      # Check tbl_flakes
      flake_check = server.succeed("psql -U crystal_forge -d crystal_forge -c \"SELECT repo_url FROM tbl_flakes WHERE repo_url = 'git+https://gitlab.com/usmcamp0811/dotfiles';\"")
      assert "git+https://gitlab.com/usmcamp0811/dotfiles" in flake_check, "flake not found in DB"

      # Check tbl_commits
      commit_check = server.succeed("psql -U crystal_forge -d crystal_forge -c \"SELECT git_commit_hash FROM tbl_commits WHERE git_commit_hash = '{commit_hash}';\"")
      assert commit_hash in commit_check, f"commit hash '{commit_hash}' not recorded in DB"

      # Ensure PostgreSQL is not active on agent
      assert "postgresql" not in agent.succeed("systemctl list-units --type=service --state=active"), "PostgreSQL is unexpectedly running on the agent"
    '';
  }
