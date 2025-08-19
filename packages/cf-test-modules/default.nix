{
  pkgs,
  lib,
  ...
}:
pkgs.python3Packages.buildPythonPackage rec {
  pname = "cf_test_modules";
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

  pythonImportsCheck = ["cf_test_modules"];

  meta = with lib; {
    description = "Modular test components for Crystal Forge integration testing";
  };
}
