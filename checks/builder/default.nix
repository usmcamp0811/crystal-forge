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
      server.wait_for_unit("postgresql")
      server.wait_for_unit("crystal-forge-server.service")
      server.wait_for_unit("crystal-forge-builder.service")
      server.succeed("systemctl is-active crystal-forge-builder.service") \
        or pytest.fail("crystal-forge-builder.service is not active")

      server.succeed("sudo -u crystal-forge nix --version") \
        or pytest.fail("crystal-forge user cannot access nix command")

      server.succeed("test -d /var/lib/crystal-forge/workdir")
      server.succeed("stat -c '%U' /var/lib/crystal-forge/workdir | grep -q crystal-forge")

      server.succeed("test -d /var/lib/crystal-forge/.cache/nix")
      server.succeed("stat -c '%U' /var/lib/crystal-forge/.cache/nix | grep -q crystal-forge")

      server.wait_until_succeeds("journalctl -u crystal-forge-builder.service | grep 'Starting Build loop'", timeout=30)

      memory_usage = server.succeed("systemctl show crystal-forge-builder.service --property=MemoryCurrent")
      if "MemoryCurrent=" in memory_usage:
          try:
              mem_bytes = int(memory_usage.split("=",1)[1].strip())
              if mem_bytes > 4 * 1024 * 1024 * 1024:
                  pytest.fail(f"Builder using excessive memory: {mem_bytes} bytes")
          except Exception:
              server.log("Warning: Could not parse MemoryCurrent")

      server.succeed("systemctl show crystal-forge-builder.service --property=MemoryMax | grep -v infinity")
      server.succeed("systemctl show crystal-forge-builder.service --property=TasksMax | grep -v infinity")

      server.succeed("systemctl reload-or-restart crystal-forge-builder.service")
      server.wait_for_unit("crystal-forge-builder.service")
      server.wait_until_succeeds("journalctl -u crystal-forge-builder.service | grep 'Starting Build loop'", timeout=30)

      server.succeed("sudo -u crystal-forge touch /var/lib/crystal-forge/workdir/result-test")
      server.succeed("sudo -u crystal-forge ln -sf /nix/store/fake /var/lib/crystal-forge/workdir/result-old")
      server.succeed("systemctl restart crystal-forge-builder.service")
      server.wait_for_unit("crystal-forge-builder.service")
      # server.wait_until_fails("test -L /var/lib/crystal-forge/workdir/result-old", timeout=30)

      server_status = server.succeed("systemctl is-active crystal-forge-server.service")
      builder_status = server.succeed("systemctl is-active crystal-forge-builder.service")
      if "active" not in server_status or "active" not in builder_status:
          pytest.fail("Server and builder services are conflicting")
    '';
  }
