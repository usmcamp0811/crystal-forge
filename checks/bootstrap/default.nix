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

        environment.etc."agent.key".source = "${keyPath}/agent.key";
        environment.etc."agent.pub".source = "${pubPath}/agent.pub";
        environment.etc."cf_flake".source = cfFlakePath;

        services.postgresql = {
          enable = true;
          authentication = lib.concatStringsSep "\n" [
            "local all root trust"
            "local all postgres peer"
            "host all all 127.0.0.1/32 trust"
            "host all all ::1/128 trust"
          ];
          initialScript = pkgs.writeText "init-crystal-forge.sql" ''
            CREATE USER crystal_forge LOGIN;
            CREATE DATABASE crystal_forge OWNER crystal_forge;
            GRANT ALL PRIVILEGES ON DATABASE crystal_forge TO crystal_forge;
          '';
        };

        services.crystal-forge = {
          enable = true;
          local-database = true;
          log_level = "debug";
          database = {
            user = "crystal_forge";
            host = "localhost";
            name = "crystal_forge";
          };
          flakes.watched = [
            {
              name = "dotfiles";
              repo_url = "https://gitlab.com/usmcamp0811/dotfiles";
              auto_poll = false;
            }
          ];
          environments = [
            {
              name = "test";
              description = "Test environment for Crystal Forge agents and evaluation";
              is_active = true;
              risk_profile = "LOW";
              compliance_level = "NONE";
            }
          ];
          systems = [
            {
              hostname = "agent";
              public_key = lib.strings.trim (builtins.readFile "${pubPath}/agent.pub");
              environment = "test";
              flake_name = "dotfiles";
            }
          ];
          server = {
            enable = true;
            host = "0.0.0.0";
            port = 3000;
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

        environment.etc."agent.key".source = "${keyPath}/agent.key";
        environment.etc."agent.pub".source = "${pubPath}/agent.pub";

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

      server.succeed("systemctl status crystal-forge-server.service || true")
      server.log("=== crystal-forge-server service logs ===")
      server.succeed("journalctl -u crystal-forge-server.service --no-pager || true")

      server.wait_for_unit("postgresql")
      server.wait_for_unit("crystal-forge-server.service")
      agent.wait_for_unit("crystal-forge-agent.service")
      server.wait_for_unit("multi-user.target")

      agent.succeed("test -r /etc/agent.key")
      agent.succeed("test -r /etc/agent.pub")
      server.succeed("test -r /etc/agent.pub")

      server.succeed("ss -ltn | grep ':3000'")
      agent.succeed("ping -c1 server")

      agent_hostname = agent.succeed("hostname -s").strip()
      system_hash = agent.succeed("readlink /run/current-system").strip().split("-")[-1]
      change_reason = "startup"

      server.wait_until_succeeds("journalctl -u crystal-forge-server.service | grep -E 'accepted.*agent'")

      output = server.succeed("psql -U crystal_forge -d crystal_forge -c 'SELECT hostname, derivation_path, change_reason FROM system_states;'")
      if agent_hostname not in output:
          pytest.fail(f"hostname '{agent_hostname}' not found in DB")
      if change_reason not in output:
          pytest.fail(f"change_reason '{change_reason}' not found in DB")
      if system_hash not in output:
          pytest.fail(f"derivation_path '{system_hash}' not found in DB")
    '';
  }
