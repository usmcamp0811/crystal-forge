{
  lib,
  inputs,
  system ? null,
  ...
}: rec {
  makeAgentNode = {
    pkgs,
    keyPath,
    pubPath,
    systemBuildClosure,
    serverHost ? "server",
    serverPort ? 3000,
    enableFirewall ? false,
    extraConfig ? {},
    ...
  }:
    {
      virtualisation.writableStore = true;
      virtualisation.memorySize = 2048;
      virtualisation.additionalPaths = [systemBuildClosure];

      environment.systemPackages = [pkgs.git pkgs.jq pkgs.crystal-forge.cf-test-suite.runTests pkgs.crystal-forge.cf-test-suite.testRunner];
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
    }
    // extraConfig;
}
