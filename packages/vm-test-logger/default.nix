{
  pkgs,
  lib,
  ...
}:
pkgs.python3Packages.buildPythonPackage rec {
  pname = "vm-test-logger";
  version = "1.0.0";
  format = "setuptools";

  src = ./.;

  propagatedBuildInputs = with pkgs.python3Packages; [
    pytest
  ];

  meta = with lib; {
    description = "Logging utilities for NixOS VM tests";
    license = licenses.mit;
    maintainers = [];
  };
}
