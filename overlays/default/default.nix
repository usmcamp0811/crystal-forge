{ channels, process-compose-flake, nixos-compose, ... }:
final: prev:
let
  overrideBatsky = batskyPkg:
    batskyPkg.overridePythonAttrs (_: {
      pyproject = true;
      build-system = [ prev.python3Packages.setuptools ];
    });

  nurRepos = prev.nur.repos // {
    kapack = prev.nur.repos.kapack // {
      batsky = overrideBatsky prev.nur.repos.kapack.batsky;
    };
  };

  nurOverlay = prev.nur.override { repos = nurRepos; };
in {
  process-compose-flake = import process-compose-flake.lib { pkgs = final; };
  nxc-lib = nixos-compose.lib;
  nxc = nixos-compose.packages.${prev.system}.nixos-compose;

  nur = nurOverlay;
}

