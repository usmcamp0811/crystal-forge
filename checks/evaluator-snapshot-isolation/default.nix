{ lib, pkgs, ... }:

# Real-Nix regression for Crystal Forge configuration snapshot extraction.
#
# Why this check exists:
# Snapshot observability is subordinate to the primary evaluation. Adding
# configuration metadata must never make a configuration fail the primary
# Crystal Forge evaluation when the ordinary toplevel/policy evaluation would
# have succeeded. The required order is:
#
#   core Nix evaluation succeeds
#     -> derivation/policy result succeeds
#     -> snapshot extraction
#          -> available snapshot
#          OR explicit partial/failed snapshot metadata
#
# The regression this check locks down:
# `walkOptions` used to force `builtins.attrNames`, `attrs.<name>`, and
# `_type` without protection. A duplicate option declaration across two
# modules is a lazy error: `config` and `system.build.toplevel` evaluate
# successfully while the merged `options` node throws when it is forced.
# Production runs `nix-eval-jobs --meta`, which forces `meta.evaluationSnapshot`
# during ordinary system evaluation, so the unguarded traversal converted a
# successful system into a confirmed Nix evaluation failure.
#
# This check evaluates the SAME prelude text that
# `build_nix_eval_expression` embeds. The text is read out of the Rust source
# so the check cannot drift from production. Renaming or removing
# `SNAPSHOT_EXTRACTION_PRELUDE` fails this check instead of silently skipping
# it.
#
# A mock evaluator cannot prove this property. The failure only appears under
# real Nix module-system laziness.

let
  source = builtins.readFile
    ../../packages/default/crates/cf-server/src/models/deployment_policies.rs;

  # Extract the production snapshot-extraction prelude verbatim.
  afterMarker =
    builtins.elemAt
      (lib.splitString ''SNAPSHOT_EXTRACTION_PRELUDE: &str = r#"'' source)
      1;
  prelude = builtins.elemAt (lib.splitString ''"#;'' afterMarker) 0;

  # The fixture reproduces the campground failure shape: two modules declare
  # `crystal-forge.stig.active`, the toplevel never reads it, and a third
  # module contributes an ordinary inspectable option.
  #
  # The text is assembled by concatenation rather than interpolation because
  # the prelude contains literal Nix `${...}` attribute selection that must not
  # be expanded by this file.
  fixtureBody = ''
    sudoModule = { lib, ... }: {
      _file = "modules/nixos/stig-modules/security/sudo/default.nix";
      options.crystal-forge.stig.active = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = "Declared by the sudo module.";
      };
    };
    pwqualityModule = { lib, ... }: {
      _file = "modules/nixos/stig-modules/security/pwquality/default.nix";
      options.crystal-forge.stig.active = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = "Declared by the pwquality module.";
      };
    };
    healthyModule = { lib, ... }: {
      _file = "modules/healthy.nix";
      options.demo.greeting = lib.mkOption {
        type = lib.types.str;
        default = "hello";
        description = "An ordinary inspectable option.";
      };
      config.demo.greeting = "hello-from-fixture";
    };

    cfg = lib.evalModules {
      modules = [ sudoModule pwqualityModule healthyModule ];
    };

    # Stands in for cfg.config.system.build.toplevel. The primary evaluation
    # result never reads the duplicated option, exactly like the reported
    # campground systems.
    drv = derivation {
      name = "crystal-forge-snapshot-isolation-toplevel";
      system = "x86_64-linux";
      builder = "/bin/false";
    };

    inputOrigins = { self = { path = null; revision = null; }; };
    rawModules = [ ];

    # Mirrors the production meta assembly in build_nix_eval_expression.
    snapshotAttempt = builtins.tryEval
      (let items = map (safeOptionSnapshot lib inputOrigins rawModules)
         (walkOptions 0 [ ] cfg.options);
       in builtins.deepSeq items items);

    snapshot = if snapshotAttempt.success then snapshotAttempt.value else [ ];
    byPath = builtins.listToAttrs
      (map (option: { name = option.path; value = option; }) snapshot);
  in {
    primaryDerivation = drv.drvPath;
    policyMetadata = { cfAgentEnabled = cfg.config.demo.greeting != ""; };
    captured = snapshotAttempt.success;
    optionCount = builtins.length snapshot;
    poisoned = byPath."crystal-forge.stig.active" or null;
    healthy = byPath."demo.greeting" or null;
  }
  '';

  fixture = builtins.toFile
    "crystal-forge-snapshot-isolation-fixture.nix"
    ("{ lib }:\nlet\n" + prelude + fixtureBody);

  result = import fixture { inherit lib; };

  poisoned = result.poisoned;
  healthy = result.healthy;
in
# 1. The primary derivation result is successful.
assert lib.assertMsg (lib.isString result.primaryDerivation
    && lib.hasSuffix ".drv" result.primaryDerivation)
  "primary evaluation must still produce a derivation path";

# 2. Policy metadata remains available alongside a degraded snapshot.
assert lib.assertMsg (result.policyMetadata.cfAgentEnabled == true)
  "policy metadata must remain available when snapshot capture degrades";

# 3. Snapshot inspection must not turn a successful configuration into an
#    evaluation error, and must not degrade the whole snapshot to unavailable
#    when the failure can be isolated to individual option nodes.
assert lib.assertMsg (result.captured == true)
  ("snapshot capture must survive an uninspectable option node; "
   + "a duplicate option declaration must be isolated per option rather than "
   + "failing or discarding the complete snapshot");

# 5. No empty snapshot may be certified as available. An empty list would
#    falsely state that the configuration declares zero options.
assert lib.assertMsg (result.optionCount > 0)
  "an empty snapshot must never be certified as available";

# 4. The uninspectable option is present and explicitly marked failed. It is
#    neither silently omitted nor fabricated as a real value.
assert lib.assertMsg (poisoned != null)
  ("the uninspectable option must appear as an explicit failed item, "
   + "not be omitted from the snapshot");
assert lib.assertMsg (poisoned.declared_type == "unknown")
  "an uninspectable option must not claim a declared type";
assert lib.assertMsg (poisoned.value.kind == "failed")
  "an uninspectable option value must be marked failed";
assert lib.assertMsg (poisoned.value.value.code == "not_evaluated")
  "an uninspectable option must report the not_evaluated code";
assert lib.assertMsg (poisoned.definitions == [ ] && poisoned.overridden == false)
  "an uninspectable option must not fabricate definition provenance";

# 6. Node-level isolation preserved every unaffected option.
assert lib.assertMsg (healthy != null)
  "options unaffected by the failure must remain visible";
assert lib.assertMsg (healthy.value.kind == "scalar"
    && healthy.value.value == "hello-from-fixture")
  "an unaffected option must retain its real evaluated value";

pkgs.runCommand "crystal-forge-evaluator-snapshot-isolation-check" { } ''
  touch "$out"
''
