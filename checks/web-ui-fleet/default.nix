{ lib, pkgs, inputs, ... }@args:
import ../web-ui/default.nix (args // {
  checkName = "web-ui-fleet";
  testProfile = "fleet";
  runAssetVerification = false;
})
