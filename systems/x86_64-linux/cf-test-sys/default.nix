{
  lib,
  pkgs,
  config,
  ...
}: {
  # This fixture is copied into integration VMs as a prebuilt deployment
  # target. It must depend only on its system configuration, not on the Git
  # revision or the complete Crystal Forge flake source.
  #
  # NixOS flake evaluation normally embeds both values: the revision appears
  # in /etc/os-release, and the `self` registry entry points at the full flake
  # source. Either value makes an unrelated web UI edit change this system and
  # therefore every VM check that preloads it. The fixture does not run Nix
  # commands against the Crystal Forge source, so both inputs are unnecessary.
  system.configurationRevision = lib.mkForce null;
  nix.registry.self.flake = lib.mkForce null;

  crystal-forge.stig-presets.off.enable = true;
  # Minimal configuration
  boot.isContainer = true;
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
