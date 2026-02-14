{ channels, process-compose-flake, nixos-compose, ... }:
final: prev:
let
  overrideBatsky = pythonPkgs:
    pythonPkgs.batsky.overridePythonAttrs (_: {
      pyproject = true;
      build-system = [ pythonPkgs.setuptools pythonPkgs.wheel ];
    });
in {
  process-compose-flake = import process-compose-flake.lib { pkgs = final; };
  nxc-lib = nixos-compose.lib;
  nxc = nixos-compose.packages.${prev.system}.nixos-compose;

  python3Packages = prev.python3Packages // {
    batsky = overrideBatsky prev.python3Packages;
  };

  batsky = overrideBatsky prev.python3Packages;
}

