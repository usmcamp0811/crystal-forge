# Native Snowfall entry point. Coordinator-owned registration point.
# WIP: only fresh local bootstrap is registered. Missing Access obligations
# are listed in docs/knowledge/testing/integration-coverage-map.md.
#
# CI: GitLab builds this normal aggregate automatically and requires it (job
# `integration-access`). The legacy integration/oidc-auth/niks3/attic/web-ui
# checks stay defined but run only as optional manual jobs. That scheduling
# change does not mean this partial owner covers their obligations.
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
