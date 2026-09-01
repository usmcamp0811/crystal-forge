{ lib, pkgs, inputs, ... }@args:
import ../web-ui/default.nix (args // {
  checkName = "web-ui-governance";
  testProfile = "governance";
  runAssetVerification = false;
})
