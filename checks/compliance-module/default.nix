# Unit tests for lib.crystal-forge.mkComplianceModule.
#
# Pure Nix evaluation: no VM, no build, no network. Every test calls the real
# helper — the same file embedded as `lib.nix` in every generated artifact —
# rather than a copy.
#
# Manifests are built with builtins.fromJSON so these tests exercise exactly the
# JSON shape the Rust generator emits and `default.nix` decodes.
{
  lib,
  pkgs,
  ...
}: let
  inherit (lib) evalModules types;

  mkComplianceModule = lib.crystal-forge.mkComplianceModule;

  # Minimal option declarations standing in for real NixOS options.
  baseModule = {lib, ...}: {
    options = {
      foo = {
        bool = lib.mkOption {
          type = types.bool;
          default = false;
        };
        int = lib.mkOption {
          type = types.int;
          default = 0;
        };
        string = lib.mkOption {
          type = types.str;
          default = "";
        };
        list = lib.mkOption {
          type = types.listOf types.str;
          default = [];
        };
        attrs = lib.mkOption {
          type = types.attrsOf types.int;
          default = {};
        };
        nested.deep.value = lib.mkOption {
          type = types.str;
          default = "untouched";
        };
      };
      some.option = lib.mkOption {
        type = types.bool;
        default = false;
      };
    };
  };

  # Build a manifest from JSON text, exactly as the generated default.nix does.
  manifestFromJSON = json: builtins.fromJSON json;

  evalWith = {
    manifest,
    baseline ? "test",
    extra ? [],
  }:
    evalModules {
      modules =
        [
          baseModule
          ({
            config,
            lib,
            ...
          }:
            mkComplianceModule {inherit config lib manifest baseline;})
        ]
        ++ extra;
    };

  assert' = cond: msg:
    if cond
    then true
    else builtins.throw "compliance-module test FAILED: ${msg}";

  # ── 1. Typed assignment conversion ─────────────────────────────────────────

  typedManifest = manifestFromJSON ''
    {
      "policies": [
        {
          "name": "typed",
          "assignments": [
            {"path": ["foo","bool"],   "value": true},
            {"path": ["foo","int"],    "value": 42},
            {"path": ["foo","string"], "value": "bar"},
            {"path": ["foo","list"],   "value": ["a","b"]},
            {"path": ["foo","attrs"],  "value": {"x": 1}}
          ]
        }
      ]
    }
  '';

  typed =
    (evalWith {
      manifest = typedManifest;
      extra = [{crystal-forge.compliance.test.enable = true;}];
    })
    .config;

  t1 = assert' (typed.foo.bool == true) "boolean assignment";
  t2 = assert' (typed.foo.int == 42) "integer assignment";
  t3 = assert' (typed.foo.string == "bar") "string assignment";
  t4 = assert' (typed.foo.list == ["a" "b"]) "list assignment";
  t5 = assert' (typed.foo.attrs == {x = 1;}) "attribute-set assignment";

  # A deep path is applied at the correct location.
  nested =
    (evalWith {
      manifest = manifestFromJSON ''
        {"policies":[{"name":"n","assignments":[
          {"path":["foo","nested","deep","value"],"value":"applied"}]}]}
      '';
      extra = [{crystal-forge.compliance.test.enable = true;}];
    })
    .config;
  t6 = assert' (nested.foo.nested.deep.value == "applied") "nested path assignment";

  # ── 2. Enable behaviour ────────────────────────────────────────────────────

  disabled =
    (evalWith {
      manifest = typedManifest;
      extra = [{crystal-forge.compliance.test.enable = false;}];
    })
    .config;
  t7 =
    assert' (disabled.foo.bool == false && disabled.foo.int == 0 && disabled.foo.string == "")
    "a disabled baseline must not apply any assignment";

  # Importing without setting enable must also apply nothing.
  notEnabled = (evalWith {manifest = typedManifest;}).config;
  t8 =
    assert' (notEnabled.foo.bool == false && notEnabled.foo.list == [])
    "importing alone must not modify the system";

  t9 =
    assert' (notEnabled.crystal-forge.compliance.test.enable == false)
    "enable must default to false";

  # ── 3. Conflict behaviour ──────────────────────────────────────────────────
  #
  # The baseline uses ordinary definitions, so a contradicting local definition
  # must produce a normal NixOS conflict rather than being silently forced.

  conflicting = evalWith {
    manifest = manifestFromJSON ''
      {"policies":[{"name":"c","assignments":[
        {"path":["some","option"],"value":false}]}]}
    '';
    extra = [
      {
        crystal-forge.compliance.test.enable = true;
        some.option = true;
      }
    ];
  };
  conflictResult = builtins.tryEval (builtins.deepSeq conflicting.config.some.option null);
  t10 =
    assert' (!conflictResult.success)
    "a local definition contradicting the baseline must produce a conflict, not a silent override";

  # An agreeing local definition is not a conflict.
  agreeing = evalWith {
    manifest = manifestFromJSON ''
      {"policies":[{"name":"a","assignments":[
        {"path":["some","option"],"value":true}]}]}
    '';
    extra = [
      {
        crystal-forge.compliance.test.enable = true;
        some.option = true;
      }
    ];
  };
  t11 = assert' (agreeing.config.some.option == true) "agreeing definitions must merge cleanly";

  # ── 4. No arbitrary evaluation ─────────────────────────────────────────────
  #
  # A manifest value that looks like Nix source must remain a literal string.

  nixLooking =
    (evalWith {
      manifest = manifestFromJSON ''
        {"policies":[{"name":"s","assignments":[
          {"path":["foo","string"],"value":"builtins.readFile /etc/passwd"}]}]}
      '';
      extra = [{crystal-forge.compliance.test.enable = true;}];
    })
    .config;
  t12 =
    assert' (nixLooking.foo.string == "builtins.readFile /etc/passwd")
    "a Nix-looking string must stay a literal string";
  t13 = assert' (builtins.isString nixLooking.foo.string) "value must remain a string";

  # ── 5. Multiple policies contribute one merged configuration ───────────────

  multi =
    (evalWith {
      manifest = manifestFromJSON ''
        {"policies":[
          {"name":"p1","assignments":[{"path":["foo","bool"],"value":true}]},
          {"name":"p2","assignments":[{"path":["foo","int"],"value":7}]}
        ]}
      '';
      extra = [{crystal-forge.compliance.test.enable = true;}];
    })
    .config;
  t14 =
    assert' (multi.foo.bool == true && multi.foo.int == 7)
    "assignments from several policies must merge";

  # A policy with no assignments is harmless.
  emptyPolicies =
    (evalWith {
      manifest = manifestFromJSON ''{"policies":[]}'';
      extra = [{crystal-forge.compliance.test.enable = true;}];
    })
    .config;
  t15 = assert' (emptyPolicies.foo.bool == false) "an empty manifest applies nothing";

  # ── 6. Baseline namespacing and provenance ─────────────────────────────────

  namespaced =
    (evalWith {
      manifest = typedManifest;
      baseline = "production-baseline";
      extra = [{crystal-forge.compliance.production-baseline.enable = true;}];
    })
    .config;
  t16 = assert' (namespaced.foo.int == 42) "a custom baseline name must work";

  summary =
    (evalWith {
      manifest = manifestFromJSON ''
        {
          "generator": "cf-nixos-module 0.3.0",
          "format_version": "2",
          "policies": [
            {"name":"p","policy_version_id":"22222222-0000-0000-0000-0000000000a1",
             "assignments":[{"path":["foo","bool"],"value":true}]}
          ],
          "bundles": [
            {"bundle_id":"aaaa","bundle_version_id":"bbbb","semantic_digest":"dddd"}
          ],
          "skipped_policies": [{"name":"skipped"}]
        }
      '';
      extra = [{crystal-forge.compliance.test.enable = true;}];
    })
    .config.crystal-forge.compliance.test.summary;

  t17 =
    assert' (summary.policyCount == 1 && summary.assignmentCount == 1)
    "summary must count policies and assignments";
  t18 =
    assert' (summary.skippedPolicyCount == 1)
    "summary must report policies that could not be converted";
  t19 =
    assert' ((builtins.head summary.bundles).bundleVersionId == "bbbb")
    "summary must expose bundle version provenance";
  t20 =
    assert' (summary.generator == "cf-nixos-module 0.3.0")
    "summary must expose the generator identity";

  # ── 7. No per-policy enable switches exist ─────────────────────────────────

  optionNames =
    builtins.attrNames
    (evalWith {manifest = typedManifest;}).options.crystal-forge.compliance.test;
  t21 =
    assert' (builtins.all (n: n == "enable" || n == "summary" || n == "_module") optionNames)
    "the baseline must expose only enable and summary, got: ${builtins.toString optionNames}";

  # ── 8. Malformed manifest data is rejected, not silently ignored ───────────

  badPath = builtins.tryEval (builtins.deepSeq
    (evalWith {
      manifest = manifestFromJSON ''
        {"policies":[{"name":"bad","assignments":[{"path":[],"value":true}]}]}
      '';
      extra = [{crystal-forge.compliance.test.enable = true;}];
    })
    .config.foo.bool
    null);
  t22 = assert' (!badPath.success) "an empty assignment path must be rejected";

  allPassed = builtins.all (x: x) [
    t1
    t2
    t3
    t4
    t5
    t6
    t7
    t8
    t9
    t10
    t11
    t12
    t13
    t14
    t15
    t16
    t17
    t18
    t19
    t20
    t21
    t22
  ];
in
  pkgs.runCommand "compliance-module-unit-tests" {
    meta.description = "Pure evaluation unit tests for lib.crystal-forge.mkComplianceModule";
  } ''
    ${lib.optionalString allPassed "echo 'All mkComplianceModule unit tests passed (22 assertions)'"}
    touch $out
  ''
