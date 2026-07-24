{ lib, pkgs, inputs, ... }:
# Unit tests for mkStigModule.
#
# These are pure Nix evaluations — no VM, no build, no network.
# They run as part of `nix flake check` and catch regressions in:
#   - Traversal: mapAttrsRecursiveCond must not recurse into override/order wrappers.
#   - Priority: STIG values use mkOverride 1 (beats mkForce, defaults, everything).
#   - Override wrappers: mkForce/mkDefault in stigConfig is unwrapped then re-wrapped.
#   - Order wrappers: mkBefore/mkAfter in stigConfig are wrapped whole (preserved).
#   - Full module eval: conflicting definitions resolve in STIG's favor.
#   - Attrset content: override wrappers whose content is an attrset (e.g., AIDE config).
#
# Every test calls lib.crystal-forge.mkStigModule directly — not a copy.
let
  inherit (lib) mkForce mkDefault mkBefore mkOverride mkIf evalModules types;

  # ── Use the PRODUCTION implementation ────────────────────────────────────
  mkStigModule = lib.crystal-forge.mkStigModule;

  # ── Shared option declarations for test scaffolding ──────────────────────
  # Each evalModules call gets these base options so mkStigModule's generated
  # options (crystal-forge.stig.*) merge cleanly.
  baseModule = { lib, ... }: {
    options = {
      assertions = lib.mkOption {
        type = types.listOf types.anything;
        default = [];
        internal = true;
      };
      warnings = lib.mkOption {
        type = types.listOf types.str;
        default = [];
        internal = true;
      };
      services.test-opt = {
        enable = lib.mkOption {
          type = types.bool;
          default = false;
          description = "Test boolean option for STIG tests";
        };
        value = lib.mkOption {
          type = types.str;
          default = "default";
          description = "Test string option for STIG tests";
        };
        attrs = lib.mkOption {
          type = types.attrsOf types.anything;
          default = {};
          description = "Test attrset option for STIG tests (like AIDE config)";
        };
      };
    };
  };

  # ── Helper: evaluate a set of modules with base options ──────────────────
  evalStig = extraModules:
    evalModules {
      modules = [ baseModule ] ++ extraModules;
    };

  # ── Helper: assert a condition, throw with a message if false ────────────
  assert' = cond: msg:
    if cond then true else builtins.throw "STIG test FAILED: ${msg}";

  # ── Test 1: Plain STIG definition beats ordinary conflicting config ─────
  # This verifies that a plain value from stigConfig (mkOverride 1) wins over
  # an ordinary module definition (priority 100).
  t1 =
    let
      result = evalStig [
        # Conflicting module that sets enable to false (normal priority)
        { services.test-opt.enable = false; }
        # STIG module that sets enable to true at priority 1
        ({ lib, config, ... }: with lib; with lib.crystal-forge; mkStigModule {
          inherit config;
          name = "testCtrl";
          stigConfig = { services.test-opt.enable = true; };
        })
      ];
    in assert' (result.config.services.test-opt.enable == true)
      "t1: STIG plain definition should override ordinary false";

  # ── Test 2: mkForce true vs mkForce false — STIG's mkForce wins ─────────
  # Both user and STIG use mkForce; STIG should win because it uses priority 1
  # which beats mkForce's priority 50.
  t2 =
    let
      result = evalStig [
        # Conflicting module that uses mkForce to set false
        { services.test-opt.enable = mkForce false; }
        # STIG module that uses mkForce to set true
        ({ lib, config, ... }: with lib; with lib.crystal-forge; mkStigModule {
          inherit config;
          name = "testCtrl";
          stigConfig = { services.test-opt.enable = mkForce true; };
        })
      ];
    in assert' (result.config.services.test-opt.enable == true)
      "t2: STIG mkForce true should beat user mkForce false";

  # ── Test 3: Direct mkBefore is preserved ─────────────────────────────────
  # Verifies that a bare mkBefore in stigConfig is NOT recursed into.
  # The order wrapper is wrapped whole at mkOverride 1.
  t3 =
    let
      result = evalStig [
        ({ lib, config, ... }: with lib; with lib.crystal-forge; mkStigModule {
          inherit config;
          name = "testCtrl";
          stigConfig = { services.test-opt.value = mkBefore "stig-value"; };
        })
      ];
    in assert' (result.config.services.test-opt.value == "stig-value")
      "t3: STIG mkBefore value should be applied correctly";

  # ── Test 4: mkDefault (mkBefore ...) — nested wrappers (pwquality pattern)
  # This is the tricky case: mkDefault wraps mkBefore. The outer mkDefault
  # (_type = "override") is encountered first and unwrapped via v.content;
  # the inner mkBefore (_type = "order") is then wrapped whole at mkOverride 1.
  t4 =
    let
      result = evalStig [
        ({ lib, config, ... }: with lib; with lib.crystal-forge; mkStigModule {
          inherit config;
          name = "testCtrl";
          stigConfig = {
            services.test-opt.value = mkDefault (mkBefore "stig-value");
          };
        })
      ];
    in assert' (result.config.services.test-opt.value == "stig-value")
      "t4: STIG mkDefault(mkBefore) value should be applied correctly";

  # ── Test 5: Override with attrset content (like AIDE mkDefault { text, mode })
  # The AIDE stig module uses mkDefault { text = "..."; mode = "0444"; }.
  # overrideAttrs must not recurse into the mkDefault wrapper; the inner
  # attrset itself is an attrset that should be recursed into normally.
  t5 =
    let
      result = evalStig [
        ({ lib, config, ... }: with lib; with lib.crystal-forge; mkStigModule {
          inherit config;
          name = "testCtrl";
          stigConfig = {
            services.test-opt.attrs = {
              text = "aide-content";
              mode = "0444";
            };
          };
        })
      ];
      val = result.config.services.test-opt.attrs;
    in assert' (val.text == "aide-content" && val.mode == "0444")
      "t5: STIG plain attrset content should preserve all keys";

  # ── Test 6: mkDefault with attrset content (AIDE pattern) ────────────────
  # Same as t5 but wrapped in mkDefault: overrideAttrs should unwrap mkDefault
  # and then recurse into the resulting attrset normally.
  t6 =
    let
      result = evalStig [
        ({ lib, config, ... }: with lib; with lib.crystal-forge; mkStigModule {
          inherit config;
          name = "testCtrl";
          stigConfig = {
            services.test-opt.attrs = mkDefault {
              text = "aide-default-content";
              mode = "0644";
            };
          };
        })
      ];
      val = result.config.services.test-opt.attrs;
    in assert' (val.text == "aide-default-content" && val.mode == "0644")
      "t6: STIG mkDefault with attrset content should preserve all keys";

  # ── Test 7: Full evalModules — regression for original mkForce crash ────
  # Reproduces the original TASK-398 crash: mkForce true inside stigConfig
  # should NOT produce a nested override wrapper that evalModules rejects.
  t7 =
    let
      result = evalStig [
        ({ lib, config, ... }: with lib; with lib.crystal-forge; mkStigModule {
          inherit config;
          name = "testCtrl";
          stigConfig = { services.test-opt.enable = mkForce true; };
        })
      ];
    in assert' (result.config.services.test-opt.enable == true)
      "t7: evalModules with mkForce in stigConfig should succeed and produce true";

  # ── Run all tests ─────────────────────────────────────────────────────────
  allPassed = t1 && t2 && t3 && t4 && t5 && t6 && t7;
in
pkgs.runCommand "stig-unit-tests" {} ''
  ${lib.optionalString allPassed "echo 'All mkStigModule unit tests passed'"}
  touch $out
''
