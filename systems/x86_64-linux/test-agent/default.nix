{
  pkgs,
  config,
  ...
}: let
  keyPair = pkgs.runCommand "agent-keypair" {} ''
    mkdir -p $out
    ${pkgs.crystal-forge.default.cf-keygen}/bin/cf-keygen -f $out/agent.key
  '';
  key = pkgs.runCommand "agent.key" {} ''
    mkdir -p $out
    cp ${keyPair}/agent.key $out/
  '';
  pub = pkgs.runCommand "agent.pub" {} ''
    mkdir -p $out
    cp ${keyPair}/agent.pub $out/
  '';
in {
  system.nixos.label = "updated-agent";
  system.stateVersion = "24.11"; # or your preferred value

  environment.etc."agent.key".source = "${key}/agent.key";
  environment.etc."agent.pub".source = "${pub}/agent.pub";

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

  environment.systemPackages = [pkgs.crystal-forge.default.agent pkgs.bash];
}
