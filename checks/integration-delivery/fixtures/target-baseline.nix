{pkgs, lib, ...}: {
  # Shared by the booted target and both authored candidates. The pinned QEMU
  # modules own root/store mounts and the test backdoor; live switches retain
  # that infrastructure. Direct-boot testing does not establish reboot support.
  system.stateVersion = "26.05";
  system.switch.enable = true;
  boot.loader.grub.enable = false;
  networking.hostName = "targetA";
  virtualisation.memorySize = 1024;
  virtualisation.diskSize = 8192;
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
      # Exercise the documented nondefault block through the real production
      # parser. TASK-497 preserves module duration strings and integer seconds.
      post_agent_start_deployment_delay = 0;
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
