{ lib, pkgs, inputs, ... }@args:
import ../web-ui/default.nix (args // {
  checkName = "web-ui-design-parity";
  testProfile = "design-parity";
  runAssetVerification = false;
  runDesignParity = true;
  blocking = false;
})
