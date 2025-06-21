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

  cf_flake =
    pkgs.runCommand "cf-flake" {
      src = ../../.;
    } ''
      mkdir -p $out
      cp -r $src/* $out/
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
        environment.etc."cf_flake".source = "${cf_flake}";

        services.postgresql = {
          enable = true;
          authentication = lib.concatStringsSep "\n" [
            "local all root trust"
            "local all postgres peer"
            "host all all 127.0.0.1/32 trust"
            "host all all ::1/128 trust"
          ];
        };
        services.crystal-forge = {
          enable = true;
          local-database = true;
          log_level = "debug";
          database = {
            user = "crystal_forge";
            host = "localhost";
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
    globalTimeout = 600;
    extraPythonPackages = p: [p.pytest];
    testScript = ''
      import pytest

      start_all()

      server.wait_for_unit("postgresql")
      server.wait_for_unit("crystal-forge-server.service")
      agent.wait_for_unit("crystal-forge-agent.service")
      server.wait_for_unit("multi-user.target")

      # Ensure keys are available
      try:
          agent.succeed("test -r /etc/agent.key")
          agent.succeed("test -r /etc/agent.pub")
          server.succeed("test -r /etc/agent.pub")
      except Exception as e:
          pytest.fail(f"Key presence check failed: {e}")

      try:
          server.succeed("ss -ltn | grep ':3000'")
      except Exception:
          pytest.fail("Server is not listening on port 3000")

      try:
          agent.succeed("ping -c1 server")
      except Exception:
          pytest.fail("Agent failed to ping server")

      agent_hostname = agent.succeed("hostname -s").strip()
      system_hash = agent.succeed("readlink /run/current-system").strip().split("-")[-1]
      context = "agent-startup"

      try:
          server.wait_until_succeeds("journalctl -u crystal-forge-server.service | grep 'accepted from agent'")
      except Exception:
          pytest.fail("Server did not log 'accepted from agent'")

      agent.log("=== agent logs ===")
      agent.log(agent.succeed("journalctl -u crystal-forge-agent.service || true"))

      output = server.succeed("psql -U crystal_forge -d crystal_forge -c 'SELECT hostname, system_derivation_id, context FROM tbl_system_states;'")
      server.log("Final DB state:\\n" + output)

      if agent_hostname not in output:
          pytest.fail(f"hostname '{agent_hostname}' not found in DB")
      if context not in output:
          pytest.fail(f"context '{context}' not found in DB")
      if system_hash not in output:
          pytest.fail(f"system_derivation_id '{system_hash}' not found in DB")

      commit_hash = "2abc071042b61202f824e7f50b655d00dfd07765"
      curl_data = f"""'{{
        "project": {{
          "web_url": "git+https://gitlab.com/usmcamp0811/dotfiles"
        }},
        "checkout_sha": "{commit_hash}"
      }}'"""

      try:
          server.succeed(f"curl -s -X POST http://localhost:3000/webhook -H 'Content-Type: application/json' -d {curl_data}")
      except Exception:
          pytest.fail("Webhook POST request failed")

      try:
          server.wait_until_succeeds(f"journalctl -u crystal-forge-server.service | grep {commit_hash}")
      except Exception:
          pytest.fail("Commit hash was not processed by server")

      flake_check = server.succeed("psql -U crystal_forge -d crystal_forge -c \"SELECT repo_url FROM tbl_flakes WHERE repo_url = 'git+https://gitlab.com/usmcamp0811/dotfiles';\"")
      if "git+https://gitlab.com/usmcamp0811/dotfiles" not in flake_check:
          pytest.fail("flake not found in DB")

      commit_list = server.succeed("psql -U crystal_forge -d crystal_forge -c 'SELECT * FROM tbl_commits;'")
      server.log("tbl_commits contents:\\n" + commit_list)

      if "0 rows" in commit_list or "0 rows" in commit_list.lower():
          pytest.fail("tbl_commits is empty")

      active_services = agent.succeed("systemctl list-units --type=service --state=active")
      if "postgresql" in active_services:
          pytest.fail("PostgreSQL is unexpectedly running on the agent")
    '';
  }
