{
  inputs,
  pkgs,
  ...
}: let
  agent-update-system = inputs.nixpkgs.lib.nixosSystem {
    system = "x86_64-linux";
    pkgs = pkgs; 
    modules = [
      ({pkgs, ...}: {
        system.nixos.label = "updated-agent";
        system.stateVersion = "24.11"; # or your preferred value

        boot.isContainer = true;
        fileSystems."/" = {
          device = "fake";
          fsType = "ext4";
        };

        networking.useDHCP = true;
        networking.firewall.enable = false;

        environment.systemPackages = [pkgs.crystal-forge.agent pkgs.bash];

        environment.etc."crystal-forge/config.toml".text = ''
          [database]
          host = "db"
          user = "crystal_forge"
          password = "password"
          dbname = "crystal_forge"
        '';

        systemd.services.agent = {
          wantedBy = ["multi-user.target"];
          after = ["network.target"];
          environment = {
            CRYSTAL_FORGE_CONFIG = "/etc/crystal-forge/config.toml";
          };
          serviceConfig = {
            ExecStart = "${pkgs.crystal-forge.agent}/bin/agent";
            Restart = "on-failure";
          };
        };
      })
    ];
  };

  agent-update = agent-update-system.config.system.build.toplevel;
in
  pkgs.testers.runNixOSTest {
    name = "crystal-forge-agent-integration";

    nodes = {
      db = {
        config,
        pkgs,
        ...
      }: {
        networking.useDHCP = true;

        services.postgresql = {
          enable = true;
          package = pkgs.postgresql_16;

          initialScript = pkgs.writeText "init.sql" ''
            CREATE USER crystal_forge WITH PASSWORD 'password';
            CREATE DATABASE crystal_forge OWNER crystal_forge;
          '';

          authentication = ''
            local   all             all                                     trust
            host    all             all             127.0.0.1/32            trust
            host    all             all             ::1/128                 trust
          '';
        };

        systemd.services.createSchema = {
          wantedBy = ["multi-user.target"];
          after = ["postgresql.service"];
          requires = ["postgresql.service"];
          serviceConfig = {
            Type = "oneshot";
            ExecStart = pkgs.writeShellScript "create-schema" ''
              cat <<'EOF' | ${pkgs.postgresql_16}/bin/psql -U crystal_forge -d crystal_forge
              CREATE TABLE system_state (
                id SERIAL PRIMARY KEY,
                hostname TEXT NOT NULL,
                system_derivation_id TEXT NOT NULL,
                inserted_at TIMESTAMPTZ NOT NULL DEFAULT now()
              );
              GRANT SELECT, INSERT, UPDATE, DELETE ON system_state TO crystal_forge;
              GRANT USAGE, SELECT ON SEQUENCE system_state_id_seq TO crystal_forge;
              EOF
            '';
          };
        };

        networking.firewall.allowedTCPPorts = [5432];
      };

      agent = {
        config,
        pkgs,
        ...
      }: {
        networking.useDHCP = true;
        networking.firewall.enable = false;

        environment.systemPackages = [pkgs.crystal-forge.agent];

        environment.etc."crystal-forge/config.toml".text = ''
          [database]
          host = "db"
          user = "crystal_forge"
          password = "password"
          dbname = "crystal_forge"
        '';

        systemd.services.agent = {
          wantedBy = ["multi-user.target"];
          after = ["network.target"];
          environment = {
            CRYSTAL_FORGE_CONFIG = "/etc/crystal-forge/config.toml";
          };
          serviceConfig = {
            ExecStart = "${pkgs.crystal-forge.agent}/bin/agent";
            Restart = "on-failure";
          };
        };
      };
    };

    testScript = ''
      start_all()

      db.wait_for_unit("postgresql")
      assert "code=exited, status=0/SUCCESS" in db.execute("systemctl status createSchema.service")[1]
      agent.wait_for_unit("agent.service")

      # Copy alternate system derivation into the VM
      agent.copy_from_host("${agent-update}", "/nix/store/updated-system")

      # Perform the switch inside the VM
      agent.succeed("/nix/store/updated-system/bin/switch-to-configuration switch")

      print(agent.succeed("readlink /run/current-system"))
      print(agent.succeed("journalctl -u agent.service --no-pager"))
    '';
  }
