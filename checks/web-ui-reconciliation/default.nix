{ lib, pkgs, inputs, ... }:
import ../web-ui/default.nix {
  inherit lib pkgs inputs;
  testSteps = "20ac-stig-import-reconciliation-fixture";
  runExportValidation = false;
  playwrightResultTimeout = 180;
}
