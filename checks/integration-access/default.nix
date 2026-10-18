# Native Snowfall entry point. Coordinator-owned registration point.
# WIP: only fresh local bootstrap is registered. Missing Access obligations
# are listed in docs/knowledge/testing/integration-coverage-map.md.
#
# CI: GitLab builds this normal aggregate automatically and requires it (job
# `integration-access`). TASK-490.24 retires legacy suite sources with explicit
# owner approval before replacement coverage is complete. This partial owner
# still proves only its 16 cases; the coverage map records missing obligations.
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
