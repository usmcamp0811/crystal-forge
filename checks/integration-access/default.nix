# Native Snowfall entry point. Coordinator-owned registration point.
# WIP: only fresh local bootstrap is registered. Missing Access obligations
# are listed in docs/knowledge/testing/integration-coverage-map.md. The legacy
# integration/oidc-auth/web-ui gates remain unchanged during construction.
{lib, inputs, pkgs, ...}: let
  harness = lib.crystal-forge.makeIntegrationTestHarness {inherit pkgs inputs;};
  scenarios = {
    lifecycle-local-bootstrap = import ./scenarios/lifecycle-local-bootstrap.nix {
      inherit harness;
      owner = "access";
      name = "lifecycle-local-bootstrap";
    };
  };
in
  pkgs.linkFarm "integration-access" (lib.mapAttrsToList (name: path: {
    inherit name path;
  }) scenarios) // {
    # Focused development: .#checks.x86_64-linux.integration-access.scenarios.<name>
    # The normal check builds every entry in scenarios; failures propagate.
    inherit scenarios;
  }
