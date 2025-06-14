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

  services.crystal-forge = {
    enable = true;
    client = {
      enable = true;
      server_host = "server";
      server_port = 3000;
      private_key = "/etc/agent.key";
    };
  };

  environment.systemPackages = [pkgs.crystal-forge pkgs.bash];
}
