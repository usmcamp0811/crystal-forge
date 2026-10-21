# Native owner aggregation: all registered mandatory scenario gates propagate
# failure. This CVE lifecycle does not imply full Assurance replacement coverage.
{lib, inputs, pkgs, ...}: let
  harness = lib.crystal-forge.makeIntegrationTestHarness {inherit pkgs inputs;};
  scenarios = {
    evidence-remediation-lifecycle = import ./scenarios/evidence-remediation-lifecycle.nix {
      inherit harness;
      owner = "assurance";
      name = "evidence-remediation-lifecycle";
    };
  };
in pkgs.linkFarm "integration-assurance" (lib.mapAttrsToList (name: path: {
  inherit name path;
}) scenarios) // {inherit scenarios;}
