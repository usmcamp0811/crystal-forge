{
  pkgs,
  lib,
  ...
}:
pkgs.python3Packages.buildPythonPackage rec {
  pname = "vm-test-logger";
  version = "1.0.0";
  format = "pyproject";

  src = ./.;

  nativeBuildInputs = with pkgs.python3Packages; [
    setuptools
    wheel
  ];

  propagatedBuildInputs = with pkgs.python3Packages; [
    pytest
  ];

  pythonImportsCheck = ["vm_test_logger"];

  meta = with lib; {
    description = "Logging utilities for NixOS VM tests";
    license = licenses.mit;
    maintainers = [];
  };
}
