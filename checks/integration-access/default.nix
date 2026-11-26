# Native Snowfall entry point. Coordinator-owned registration point.
# WIP: local bootstrap, sessions, role authorization and Admin form persistence
# share one registered scenario. Missing Access obligations
# are listed in docs/knowledge/testing/integration-coverage-map.md.
#
# CI: GitLab runs each registered scenario gate as a required job in the
# generated child pipeline (packages/ci/integration-pipeline.nix). Legacy
# suite retirement has explicit owner approval before replacement coverage is
# complete (TASK-490.24). This partial owner proves selected behaviors; the
# coverage map records missing obligations.
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
