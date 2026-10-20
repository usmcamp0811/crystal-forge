{pkgs, lib, ...}: {
  # Shared by the booted target and both authored candidates. The pinned QEMU
  # modules own root/store mounts and the test backdoor; live switches retain
  # that infrastructure. Direct-boot testing does not establish reboot support.
  system.stateVersion = "26.05";
  system.switch.enable = true;
  boot.loader.grub.enable = false;
  networking.hostName = "targetA";
  virtualisation.memorySize = 1024;
  services.crystal-forge = {
    enable = true;
    local-database = false;
    server.enable = false;
    build.enable = false;
    client = {
      enable = true;
      server_host = "controller";
      server_port = 3000;
      private_key = "/run/cf-it-agent.key";
    };
    deployment = {
      deployment_strategy = "immediate_persist";
      # Retain the production default startup delay. Nondefault block emission
      # currently exposes the separate TASK-497 polling-interval type defect.
      fallback_to_local_build = false;
      require_sigs = true;
    };
  };
  systemd.services.crystal-forge-agent.wantedBy = lib.mkForce [];
  systemd.services.crystal-forge-agent.path = [pkgs.openssh];
  environment.systemPackages = [pkgs.curl pkgs.jq pkgs.openssh];
  nix.settings = {
    experimental-features = ["nix-command" "flakes"];
    substituters = lib.mkForce [];
    builders = lib.mkForce [];
    fallback = false;
    require-sigs = true;
  };
}
