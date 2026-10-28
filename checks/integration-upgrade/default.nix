# One real version transition, registered through the existing owner mechanism.
{pkgs, lib, inputs, ...}: let
  harness = lib.crystal-forge.makeIntegrationTestHarness {inherit pkgs inputs;};
  scenarios.version-transition = import ./scenarios/version-transition.nix {
    inherit harness;
    owner = "upgrade";
    name = "version-transition";
  };
in (pkgs.linkFarm "integration-upgrade" (lib.mapAttrsToList (name: path: {
  inherit name path;
}) scenarios)) // {inherit scenarios;}
