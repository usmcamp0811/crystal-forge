{
  pkgs,
  config,
  ...
}: {
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
}
