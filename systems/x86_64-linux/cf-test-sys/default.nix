{
  lib,
  pkgs,
  config,
  ...
}: {
  # Minimal configuration
  # boot.isContainer = true;
  fileSystems."/" = {
    device = "none";
    fsType = "tmpfs";
  };
  services.getty.autologinUser = "root";
  environment.systemPackages = [];
  system.stateVersion = "25.05";

  # Disable unnecessary services for faster build
  services.udisks2.enable = false;
  security.polkit.enable = false;
  documentation.enable = false;
  documentation.nixos.enable = false;

  # Disable NSS modules instead of nscd to avoid the assertion error
  system.nssModules = lib.mkForce [];
}
