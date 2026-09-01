{ lib, pkgs, inputs, ... }@args:
import ../web-ui/default.nix (args // {
  checkName = "web-ui-pipeline";
  testProfile = "pipeline";
  runAssetVerification = false;
})
