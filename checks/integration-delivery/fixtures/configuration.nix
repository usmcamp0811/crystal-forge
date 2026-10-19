{lib, sourceNonce, ...}: {
  # A real NixOS configuration. Only the toplevel builder command changes with
  # the runtime-authored nonce; baseline preparation cannot supply its output.
  system.stateVersion = "26.05";
  networking.hostName = "targetA";
  boot.loader.grub.enable = false;
  fileSystems."/" = {device = "none"; fsType = "tmpfs";};
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
  };
  system.systemBuilderCommands = ''
    printf '%s' ${lib.escapeShellArg sourceNonce} > "$out/cf-it-source-nonce"
  '';
}
