# Explicit behavior-oriented lifecycle registration. No packet-specific gates.
{pkgs, lib, inputs, ...}: let
  harness = lib.crystal-forge.makeIntegrationTestHarness {inherit pkgs inputs;};
  scenarios = {
    delivery-lifecycle = import ./scenarios/delivery-lifecycle.nix {
      inherit harness;
      owner = "delivery";
      name = "delivery-lifecycle";
    };
  };
in (pkgs.linkFarm "integration-delivery" (lib.mapAttrsToList (name: path: {
  inherit name path;
}) scenarios)) // {inherit scenarios;}
