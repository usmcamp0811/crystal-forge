# Small shared NixOS test helpers for the four integration checks.
#
# Contract: docs/knowledge/testing/integration-test-contract.md.
# Coverage and retirement map: docs/knowledge/testing/integration-coverage-map.md.
#
# Snowfall exports makeIntegrationTestHarness from ./default.nix. Native
# checks/integration-<owner>/default.nix files register scenarios explicitly.
#
# INVARIANT: Every scenario runs the production packages selected in
# `packages` below through the normal `services.crystal-forge` NixOS module.
# The harness MUST NOT patch, override, or replace application logic.
{
  pkgs,
  lib,
  inputs,
}: let
  # Production component builds. `cf-server-drv` embeds the shipped WASM UI.
  # The core server build has no UI and MUST NOT be used by a browser scenario.
  packages = {
    server = pkgs.crystal-forge.default.cf-server-drv;
    agent = pkgs.crystal-forge.default.cf-agent-drv;
    builder = pkgs.crystal-forge.default.cf-builder-drv;
  };

  # The five permitted VM roles. A scenario instantiates only the roles that
  # it needs. INVARIANT: no scenario defines a node outside this list, so the
  # contract's five-VM maximum holds for every owner.
  roleNames = ["controller" "services" "builderA" "builderB" "targetA"];

  factLib = import ./facts.nix {inherit pkgs lib inputs;};
  browser = import ./browser.nix {inherit pkgs lib inputs;};
  roles = import ./roles.nix {
    inherit pkgs lib inputs packages;
    browserLib = browser;
  };

  driverPrelude = ./driver.py;

  # Builds one scenario: an evidence-producing VM run plus a gate.
  #
  # Arguments:
  #   owner      - "access" | "delivery" | "assurance" | "upgrade".
  #   name       - scenario name; equals the file name under <owner>/scenarios.
  #   contracts  - contract IDs from the coverage map that this scenario proves.
  #   nodes      - attrset of role name -> NixOS module. Keys MUST be roles.
  #   facts      - authored input facts (JSON-serializable attrset). They are
  #                the independent expected values; see lib/facts.nix.
  #   script     - path to the Python scenario body (runs in the test driver).
  #   browserSpecs - attrset of case name -> CommonJS browser spec file.
  #   timeout    - global VM test timeout in seconds.
  #
  # Result: the gate derivation. `passthru.run` is the evidence run.
  #
  # INVARIANT: the `.run` derivation records failures instead of raising so
  # that its artifacts survive a failed scenario. It is evidence only and MUST
  # NOT be registered as a check. The gate reads `result.json` and fails unless
  # the status is exactly "passed". Driver crashes and global timeouts fail
  # the run derivation itself, so the gate also fails.
  mkScenario = {
    owner,
    name,
    contracts,
    nodes,
    facts ? {},
    script,
    browserSpecs ? {},
    timeout ? 900,
  }: let
    unknownRoles = lib.subtractLists roleNames (builtins.attrNames nodes);
    factsFile = pkgs.writeText "cf-it-${owner}-${name}-facts.json" (builtins.toJSON facts);
    specsDir = pkgs.linkFarm "cf-it-${owner}-${name}-specs" (lib.mapAttrsToList (case: path: {
        name = "${case}.cjs";
        inherit path;
      })
      browserSpecs);
    meta = pkgs.writeText "cf-it-${owner}-${name}-meta.json" (builtins.toJSON {
      inherit owner name contracts;
      sourceRevision = inputs.self.rev or inputs.self.dirtyRev or "unknown";
      renderProfile = browser.profile;
      design = factLib.designReference;
    });
    run = pkgs.testers.runNixOSTest {
      name = "cf-it-${owner}-${name}-run";
      skipLint = true;
      skipTypeCheck = true;
      globalTimeout = timeout;
      nodes =
        lib.mapAttrs (role: module: {
          imports = [module (roles.common role)];
          environment.etc."cf-it/facts.json".source = factsFile;
          environment.etc."cf-it/specs".source = specsDir;
        })
        nodes;
      testScript = ''
        CF_IT_META = "${meta}"
        CF_IT_FACTS = "${factsFile}"
        CF_IT_BODY = "${script}"
        exec(open("${driverPrelude}").read())
      '';
    };
  in
    assert lib.assertMsg (unknownRoles == [])
    "cf-it ${owner}/${name}: unknown role(s) ${toString unknownRoles}; allowed: ${toString roleNames}";
    assert lib.assertMsg (contracts != [])
    "cf-it ${owner}/${name}: a scenario must name at least one contract ID";
      pkgs.runCommand "cf-it-${owner}-${name}" {
        nativeBuildInputs = [pkgs.jq];
        passthru = {inherit run owner name contracts;};
      } ''
        mkdir -p $out
        ln -s ${run} $out/evidence
        status=$(jq -r '.status // "missing"' ${run}/result.json 2>/dev/null || echo missing)
        echo "cf-it ${owner}/${name}: status=$status evidence=${run}"
        jq -r '.cases[]? | "  case \(.name): \(.status) (\(.seconds)s)"' ${run}/result.json || true
        if [ "$status" != "passed" ]; then
          echo "cf-it ${owner}/${name}: FAILED" >&2
          jq -r '.error // "no error recorded"' ${run}/result.json >&2 || true
          echo "Artifacts: ${run}" >&2
          exit 1
        fi
      '';

  self = {
    inherit packages roleNames roles browser mkScenario pkgs lib inputs;
    facts = factLib;
  };
in
  self
