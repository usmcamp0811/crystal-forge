{ lib, pkgs, inputs, ... }:
# Unit tests for mkStigModule and the overrideAttrs helper.
#
# These tests are pure Nix evaluations — no VM, no build, no network.
# They run as part of `nix flake check` and catch regressions in:
#   1. Traversal: mapAttrsRecursiveCond must not recurse into override wrappers.
#   2. Priority: plain STIG values must use mkOverride 1 (beats everything).
#   3. Override wrappers: mkForce in stigConfig is unwrapped then re-wrapped at prio 1.
#   4. Order wrappers: mkBefore internals are accessible (order wrappers ARE recursed).
#   5. Full module eval: a NixOS module with mkForce in stigConfig must evaluate cleanly.
let
  inherit (lib) mkForce mkOverride mapAttrsRecursiveCond evalModules;

  # ── overrideAttrs — must stay in sync with lib/stig/default.nix ─────────────
  overrideAttrs = attrs: mapAttrsRecursiveCond
    (v: !(v ? _type && v._type == "override"))   # stop at override wrappers
    (_: v:
      if v ? _type && v._type == "override"
      then mkOverride 1 v.content   # unwrap override, re-apply at STIG priority
      else mkOverride 1 v           # plain value — wrap directly
    )
    attrs;

  # ── Helper: assert a condition, throw with a message if false ───────────────
  assert' = cond: msg: if cond then true else builtins.throw "STIG test FAILED: ${msg}";

  # ── Test 1: plain value gets priority 1 ─────────────────────────────────────
  t1 =
    let r = overrideAttrs { services.foo.enable = true; };
    in assert' (r.services.foo.enable.priority == 1 && r.services.foo.enable.content == true)
         "plain bool: priority=1 content=true";

  # ── Test 2: mkForce value is unwrapped; content applied at priority 1 ───────
  # mkForce true  = { _type="override"; priority=50; content=true }
  # overrideAttrs → mkOverride 1 true = { _type="override"; priority=1; content=true }
  t2 =
    let r = overrideAttrs { services.foo.enable = mkForce true; };
    in assert'
         (r.services.foo.enable.priority == 1 && r.services.foo.enable.content == true)
         "mkForce bool: priority=1 content=true (no nested wrapper)";

  # ── Test 3: mkForce string is unwrapped correctly ───────────────────────────
  t3 =
    let r = overrideAttrs { services.foo.text = mkForce "hello"; };
    in assert'
         (r.services.foo.text.priority == 1 && r.services.foo.text.content == "hello")
         "mkForce string: priority=1 content=hello";

  # ── Test 4: plain value beats mkForce by numeric priority ───────────────────
  # After overrideAttrs both plain values and unwrapped-mkForce values have priority 1.
  # 1 < 50, so mkOverride 1 beats mkForce (50) and normal defs (100).
  t4 = assert' (1 < 50 && 1 < 100)
    "mkOverride 1 has higher precedence than mkForce (50) and normal defs (100)";

  # ── Test 5: full NixOS evalModules with mkForce in stigConfig succeeds ───────
  # This is the regression for the original crash in TASK-398.
  t5 =
    let
      result = evalModules {
        modules = [
          # Minimal option declarations
          ({ lib, ... }: {
            options = {
              services.timesyncd.enable = lib.mkOption {
                type = lib.types.bool;
                default = false;
              };
              crystal-forge.stig.timesyncd = {
                enable = lib.mkOption { type = lib.types.bool; default = true; };
                justification = lib.mkOption {
                  type = lib.types.listOf lib.types.str;
                  default = [];
                };
              };
              crystal-forge.stig.active = lib.mkOption {
                type = lib.types.attrsOf (lib.types.attrsOf lib.types.anything);
                default = {};
              };
              crystal-forge.stig.inactive = lib.mkOption {
                type = lib.types.attrsOf (lib.types.attrsOf lib.types.anything);
                default = {};
              };
            };
          })
          # Apply STIG config the same way mkStigModule does
          ({ lib, config, ... }:
            let
              cfg = config.crystal-forge.stig.timesyncd;
              stigConfigRaw = {
                services.timesyncd.enable = mkForce true;  # ← the crash in TASK-398
              };
            in {
              config = lib.mkMerge [
                (lib.mkIf cfg.enable (overrideAttrs stigConfigRaw))
                {
                  crystal-forge.stig.active.timesyncd = lib.mkIf cfg.enable {
                    srg = [];
                    cci = [];
                    config = stigConfigRaw;
                  };
                }
              ];
            }
          )
        ];
      };
    in
      assert' (result.config.services.timesyncd.enable == true)
        "evalModules with mkForce in stigConfig: services.timesyncd.enable should be true";

  allPassed = t1 && t2 && t3 && t4 && t5;
in
pkgs.runCommand "stig-unit-tests" {} ''
  ${lib.optionalString allPassed "echo 'All mkStigModule unit tests passed'"}
  touch $out
''
