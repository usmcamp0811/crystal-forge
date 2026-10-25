# Both the installed target and source-built candidates retain the exact release
# agent until the operator explicitly selects the current specialisation.
{predecessor, currentModule, currentAgent, sourceNonce, useCurrentAgent ? false}:
{pkgs, lib, ...}: {
  imports = [
    (if useCurrentAgent then currentModule else import ./released-module.nix {inherit predecessor;})
    ../../integration-delivery/fixtures/target-network.nix
    ../../integration-delivery/fixtures/network-isolation.nix
  ];
  system.stateVersion = "26.05";
  system.switch.enable = true;
  boot.loader.grub.enable = false;
  networking.hostName = "targetA";
  virtualisation.memorySize = 1024;
  virtualisation.diskSize = 8192;
  # The release's tmpfiles rules name this service account on agent-only hosts.
  users.users.crystal-forge = {isSystemUser = true; group = "crystal-forge";};
  services.crystal-forge = {
    enable = true;
    local-database = false;
    server.enable = false;
    build.enable = false;
    client = {
      enable = true;
      server_host = "controller";
      server_port = 3000;
      private_key = "/var/lib/upgrade-identity/agent.key";
    } // lib.optionalAttrs useCurrentAgent {package = currentAgent;};
    deployment = {
      deployment_strategy = "immediate_persist";
      fallback_to_local_build = false;
      require_sigs = true;
      # Keep the release's ordinary deployment defaults. Its module emits
      # duration strings for a nondefault block, but this exact agent expects
      # integer seconds there. Current config parsing has a separate contract.
    } // lib.optionalAttrs useCurrentAgent {
      deployment_poll_interval = "2s";
      post_agent_start_deployment_delay = 0;
    };
  };
  systemd.services.crystal-forge-agent.wantedBy = lib.mkForce [];
  systemd.services.crystal-forge-agent.path = [pkgs.openssh];
  environment.systemPackages = [pkgs.curl pkgs.jq pkgs.openssh predecessor.packages.${pkgs.stdenv.hostPlatform.system}.cf-keygen];
  environment.etc."cf-it-revision".text = sourceNonce;
  system.systemBuilderCommands = ''
    ${lib.optionalString (lib.hasPrefix "interrupted-" sourceNonce) ''
      printf '%s\n' 'upgrade-interrupted-build-started' >&2
      ${pkgs.coreutils}/bin/sleep 120
    ''}
    printf '%s' ${lib.escapeShellArg sourceNonce} > "$out/cf-it-source-nonce"
  '';
  nix.settings = {
    experimental-features = ["nix-command" "flakes"];
    substituters = lib.mkForce [];
    builders = lib.mkForce [];
    fallback = false;
  };
  specialisation = lib.mkIf (!useCurrentAgent) {current.configuration = {
    disabledModules = [{key = "upgrade-released-module";}];
    imports = [currentModule];
    services.crystal-forge.client.package = currentAgent;
    services.crystal-forge.deployment.post_agent_start_deployment_delay = 0;
    services.crystal-forge.deployment.deployment_poll_interval = "2s";
  };};
}
