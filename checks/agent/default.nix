{pkgs, ...}:
pkgs.testers.runNixOSTest {
  name = "crystal-forge-agent-integration";

  nodes = {
    db = {
      config,
      pkgs,
      ...
    }: {
      services.postgresql = {
        enable = true;
        package = pkgs.postgresql_16;
        initialScript = pkgs.writeText "init.sql" ''
          CREATE USER crystal_forge WITH PASSWORD 'password';
          CREATE DATABASE crystal_forge OWNER crystal_forge;
          \\c crystal_forge
          CREATE TABLE system_state (
            id SERIAL PRIMARY KEY,
            hostname TEXT NOT NULL,
            system_derivation_id TEXT NOT NULL,
            inserted_at TIMESTAMPTZ NOT NULL DEFAULT now()
          );
          GRANT SELECT, INSERT, UPDATE, DELETE ON system_state TO crystal_forge;
          GRANT USAGE, SELECT ON SEQUENCE system_state_id_seq TO crystal_forge;
        '';
      };
      networking.firewall.allowedTCPPorts = [5432];
    };

    agent = {
      config,
      pkgs,
      ...
    }: {
      environment.systemPackages = [
        pkgs.crystal-forge.agent
      ];

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
        serviceConfig = {
          ExecStart = "${pkgs.crystal-forge.agent}/bin/agent";
          Environment = "CRYSTAL_FORGE_CONFIG=/etc/crystal-forge/config.toml";
        };
      };

      networking.firewall.enable = false;
      networking.hosts = {
        "127.0.0.1" = ["agent"];
      };
    };
  };

  testScript = ''
    start_all()
    db.wait_for_unit("postgresql")
    agent.wait_for_unit("agent.service")

    result = agent.succeed("readlink /run/current-system")

    retry = 0
    while retry < 10:
        out = db.succeed("psql -U crystal_forge -d crystal_forge -c 'SELECT * FROM system_state'")
        if result.strip() in out:
            break
        retry += 1
        time.sleep(1)
    else:
        fail("system_state not updated as expected")
  '';
}
