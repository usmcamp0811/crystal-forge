{ channels, process-compose-flake, nixos-compose, ... }:
final: prev: {
  process-compose-flake = import process-compose-flake.lib { pkgs = final; };
  nxc-lib = nixos-compose.lib;
  nxc = nixos-compose.packages.${prev.system}.nixos-compose;

  python3Packages = prev.python3Packages.override {
    packageOverrides = pythonFinal: pythonPrev: {
      batsky = pythonPrev.batsky.overridePythonAttrs (old: {
        pyproject = true;
        build-system = with pythonPrev; [ setuptools wheel ];
      });
    };
  };
}
