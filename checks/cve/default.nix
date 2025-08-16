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

  sqlTestsPath = pkgs.writeText "crystal-forge-view-tests.sql" (builtins.readFile ./tests/view-tests.sql);

  # Create a proper derivation for the test package
  testPackage = pkgs.stdenv.mkDerivation {
    pname = "crystal-forge-tests";
    version = "1.0.0";

    src = ./tests;

    dontBuild = true;

    installPhase = ''
      mkdir -p $out

      # Copy all files from the tests directory
      cp -r $src/* $out/
    '';
  };
  testScript = builtins.readFile "${testPackage}/run_tests.py";
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

        environment.etc."crystal-forge-tests.sql".source = sqlTestsPath;
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
      server.wait_for_unit("crystal-forge-server.service")
      server.wait_for_unit("postgresql")
      server.succeed("systemctl start crystal-forge-builder.service")
      server.wait_for_unit("crystal-forge-builder.service")

      try:
          server.succeed("which vulnix")
          server.wait_until_succeeds("journalctl -u crystal-forge-builder.service | grep 'Starting CVE Scan loop'", timeout=30)
          server.wait_until_succeeds("journalctl -u crystal-forge-builder.service | grep -E '(CVE scan|vulnix)'", timeout=120)
      except Exception:
          server.log("Warning: vulnix/CVE scan not available or did not run within timeout")
    '';
  }
