{
  description = "Minimal test systems for cf-test-sys and test-agent";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";

  outputs = {
    self,
    nixpkgs,
  }: let
    system = "x86_64-linux";
    pkgs = import nixpkgs {inherit system;};
    lib = nixpkgs.lib;

    minimalConfig = {
      lib,
      pkgs,
      config,
      ...
    }: {
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
    };
  in {
    nixosConfigurations = {
      cf-test-sys = nixpkgs.lib.nixosSystem {
        inherit system;
        modules = [minimalConfig];
      };

      test-agent = nixpkgs.lib.nixosSystem {
        inherit system;
        modules = [minimalConfig];
      };
    };
  };
}
