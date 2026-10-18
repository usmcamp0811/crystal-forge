{
  outputs = { self }:
    let
      hostEnvironment = builtins.getEnv "CF_VERIFIED_SOURCE_HOST_VALUE";
      hostPath = builtins.pathExists "/tmp/crystal-forge-verified-source-host-path";
      sourceClass =
        if hostEnvironment == "" && !hostPath then "pure" else "impure";
      system = builtins.derivation {
        name = "verified-source-${sourceClass}";
        system = "x86_64-linux";
        builder = "/bin/sh";
        args = [ "-c" "touch $out" ];
      };
    in {
      packages.x86_64-linux.default = system;
      nixosConfigurations.host.config = {
        system.build.toplevel = system;
        systemd.services.crystal-forge-agent.enable = true;
        services.crystal-forge = {
          enable = false;
          client.enable = false;
        };
      };
    };
}
