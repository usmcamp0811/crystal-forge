{ lib, pkgs, inputs, ... }@args:
import ../web-ui/default.nix (args // {
  checkName = "web-ui-exports";
  testProfile = "design-parity";
  runAssetVerification = false;
  runBrowserSemanticValidation = false;
  runExportValidation = true;
  gateBrowserValidation = false;
})
