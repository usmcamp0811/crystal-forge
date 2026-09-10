{ pkgs, ... }:

let
  inspectorSource = builtins.readFile ../../packages/default/crates/cf-server/src/models/config_inspector.nix;
  definitionValuesSource = builtins.readFile ../../packages/default/crates/cf-server/src/models/config_definition_values.nix;
  provenanceSource = builtins.readFile ../../packages/default/crates/cf-server/src/models/config_provenance.nix;
  provenanceLibSource = builtins.readFile ../../packages/default/crates/cf-server/src/models/config_provenance_lib.nix;
  valueEncodingSource = builtins.readFile ../../packages/default/crates/cf-server/src/models/config_value_encoding.nix;
  legacyNixpkgs = builtins.fetchTree
    (builtins.fromJSON
      (builtins.readFile ../../lib/test-flake/test-flake/flake.lock)).nodes.nixpkgs.locked;
  fixture = pkgs.runCommand "crystal-forge-config-inspector-fixture" { } ''
    mkdir -p "$out"
    touch "$out/explicit-modules-location.nix"
    cat > "$out/flake.nix" <<'EOF'
    {
      inputs.base.url = "path:${pkgs.path}";

      outputs = { self, base, ... }:
        let
           lib = base.lib;
           child = { lib, ... }: {
             config.crystalForgeProbe = {
               samePriority = lib.mkOverride 400 "child";
               poison = lib.mkForce "poison-winner";
               imported = "child";
             };
           };
           duplicate = { ... }: {
             config.crystalForgeProbe.duplicate = "once";
           };
           priorityOrdinary = { ... }: {
             config.crystalForgeProbe.priorityProbe = "ordinary";
           };
           priorityDefault = { lib, ... }: {
             config.crystalForgeProbe.priorityProbe = lib.mkDefault "default";
           };
           priorityOverride = { lib, ... }: {
             config.crystalForgeProbe.priorityProbe = lib.mkOverride 500 "override";
           };
           priorityForce = { lib, ... }: {
             config.crystalForgeProbe.priorityProbe = lib.mkForce "force";
           };
           disabled = {
             key = "disabled-module";
             config.crystalForgeProbe.disabled = "must-not-survive";
           };
           disabler = {
             key = "disabler";
             disabledModules = [ { key = "disabled-module"; } ];
             imports = [ disabled ];
           };
           module = { lib, pkgs, ... }: {
                imports = [
                  child duplicate duplicate
                  priorityOrdinary priorityDefault priorityOverride priorityForce
                ];
             options.crystalForgeProbe = {
              healthyBefore = lib.mkOption {
                type = lib.types.str;
                default = "before";
              };
              poison = lib.mkOption {
                type = lib.types.str;
                default = "poison";
                apply = _: throw "selected poisoned option forced";
              };
               healthyAfter = lib.mkOption {
                 type = lib.types.str;
                 default = "after";
               };
               samePriority = lib.mkOption {
                 type = lib.types.str;
                 default = "unset";
               };
               imported = lib.mkOption {
                 type = lib.types.str;
                 default = "unset";
               };
               conditionalTrue = lib.mkOption {
                 type = lib.types.str;
                 default = "unset";
               };
               conditionalFalse = lib.mkOption {
                 type = lib.types.str;
                 default = "unset";
               };
               merged = lib.mkOption {
                 type = lib.types.listOf lib.types.str;
                 default = [ ];
               };
               ordered = lib.mkOption {
                 type = lib.types.listOf lib.types.str;
                 default = [ ];
               };
                duplicate = lib.mkOption {
                  type = lib.types.str;
                  default = "unset";
                };
                priorityProbe = lib.mkOption {
                  type = lib.types.str;
                  default = "unset";
                };
               disabled = lib.mkOption {
                 type = lib.types.str;
                 default = "unset";
               };
             };

             config = {
               crystalForgeProbe.healthyBefore = "before";
               crystalForgeProbe.healthyAfter = "after";
               crystalForgeProbe.poison = lib.mkDefault "poison-loser";
               crystalForgeProbe.samePriority = lib.mkOverride 400 "module";
               crystalForgeProbe.conditionalTrue = lib.mkIf true "true";
               crystalForgeProbe.conditionalFalse = lib.mkIf false "false";
               crystalForgeProbe.merged = lib.mkMerge [ [ "first" ] [ "second" ] ];
               crystalForgeProbe.ordered = lib.mkMerge [
                 (lib.mkOrder 200 [ "late" ])
                 (lib.mkOrder 100 [ "early" ])
               ];
              boot.loader.grub.devices = [ "nodev" ];
              fileSystems."/" = {
                device = "/dev/sda";
                fsType = "ext4";
              };
              boot.consoleLogLevel = 7;
              boot.kernelParams = [ "quiet" ];
               environment.systemPackages = [ pkgs.hello ];
               system.stateVersion = "26.05";
             };
          };
        in {
           nixosConfigurations.good = lib.nixosSystem {
             system = builtins.currentSystem;
             modulesLocation = toString ./explicit-modules-location.nix;
              modules = [ module disabler ];
           };
            nixosConfigurations.partial =
              let evaluated = lib.nixosSystem {
                system = builtins.currentSystem;
                modulesLocation = toString ./explicit-modules-location.nix;
                modules = [ module disabler ];
              };
                  longComponent = lib.concatStrings (lib.replicate 257 "x");
               in evaluated // {
                 # Model a delayed module-system declaration failure without
                 # invalidating config.system.build.toplevel or sibling options.
                 # The scalar children model malformed but readable option-tree
                 # entries that must not disappear silently.
                options = evaluated.options // {
                  "api_token=typed-diagnostic-secret" = "not an option subtree";
                  ''${longComponent} = "not an option subtree";
                  crystalForgeProbe = evaluated.options.crystalForgeProbe // {
                    poison = throw "delayed module-system declaration subtree poisoned";
                   };
                };
              };
           nixosConfigurations.diagnosticStress =
             let
               evaluated = lib.nixosSystem {
                 system = builtins.currentSystem;
                 modules = [ module disabler ({ lib, ... }: {
                   options.crystalForgeStress.zzzHealthy = lib.mkOption {
                     type = lib.types.str;
                   };
                   config.crystalForgeStress.zzzHealthy = "healthy-after-budget";
                 }) ];
               };
               poison = builtins.listToAttrs (builtins.genList (index: {
                 name = "poison''${lib.fixedWidthString 3 "0" (toString index)}";
                 value = throw "diagnostic stress poison ''${toString index}";
               }) 130);
              in evaluated // {
               options = {
                 crystalForgeStress = poison // {
                   inherit (evaluated.options.crystalForgeStress) zzzHealthy;
                 };
               };
             };
           nixosConfigurations.unrelatedBroken =
             throw "unrelated configuration forced";
          nixosModules.unrelatedBroken = { lib, ... }:
            with lib.namespace-change-me;
            { namespace-change-me.enable = true; };
        };
    }
    EOF
  '';

  legacyFixture = pkgs.runCommand "crystal-forge-config-inspector-legacy-fixture" { } ''
    mkdir -p "$out"
    cat > "$out/flake.nix" <<'EOF'
    {
      inputs.base.url = "path:${legacyNixpkgs.outPath}";

      outputs = { base, ... }: {
        nixosConfigurations.legacy = base.lib.nixosSystem {
          system = builtins.currentSystem;
          modules = [
            ({ lib, ... }: {
              options.crystalForgeProbe.enabled = lib.mkEnableOption "legacy probe";
              config = {
                crystalForgeProbe.enabled = true;
                boot.isContainer = true;
                fileSystems."/" = {
                  device = "none";
                  fsType = "tmpfs";
                };
                system.stateVersion = "25.05";
              };
             })
          ];
        };
      };
    }
    EOF
  '';

  expression = ''
    let
      flakeRef = "path:${fixture}";
      configurationName = "good";
       flake = builtins.getFlake flakeRef;
       configuration = builtins.getAttr configurationName flake.nixosConfigurations;
       targetKey = builtins.hashString "sha256" (builtins.toJSON [ flakeRef configurationName ]);
       valueEncoder = (${valueEncodingSource});
         inspector = (${inspectorSource}) {
           inherit flake configuration targetKey;
          provenanceLib = (${provenanceLibSource});
          encodeValue = valueEncoder configuration.pkgs.lib;
        };
    in inspector
  '';

  unsupportedCapabilityExpression = ''
    let
      nixpkgs = import ${pkgs.path} { };
      fakeLib = nixpkgs.lib // {
        modules = builtins.removeAttrs nixpkgs.lib.modules [ "pushDownProperties" ];
      };
      configuration = {
        pkgs = nixpkgs // { lib = fakeLib; };
        config.system.build.toplevel = nixpkgs.runCommand "crystal-forge-unsupported-provenance" { } "touch $out";
        options = { };
        graph = [ ];
        _module.args.moduleType.functor.payload = {
          modules = [ ];
          specialArgs = { };
          class = "nixos";
        };
      };
    in ((${provenanceSource}) {
      flake = { inputs = { }; };
      inherit configuration;
      provenanceLib = (${provenanceLibSource});
    }).meta.crystalForgeProvenance
  '';

  partialExpression = ''
    let
      flakeRef = "path:${fixture}";
      configurationName = "partial";
      flake = builtins.getFlake flakeRef;
      configuration = builtins.getAttr configurationName flake.nixosConfigurations;
      targetKey = builtins.hashString "sha256" (builtins.toJSON [ flakeRef configurationName ]);
      valueEncoder = (${valueEncodingSource});
      inspector = (${inspectorSource}) {
        inherit flake configuration targetKey;
        provenanceLib = (${provenanceLibSource});
        encodeValue = valueEncoder configuration.pkgs.lib;
      };
    in inspector
  '';

  rootFailureExpression = ''
    let
      nixpkgs = import ${pkgs.path} { };
      configuration = {
        pkgs = nixpkgs;
        config.system.build.toplevel = nixpkgs.runCommand "config-inspector-root-failure" { } "touch $out";
        options = throw "option inventory root poisoned";
      };
      jobs = (${inspectorSource}) {
        flake = { outPath = "/nix/store/root-failure-source"; inputs = { }; };
        inherit configuration;
        targetKey = builtins.hashString "sha256" "root-failure";
        provenanceLib = (${provenanceLibSource});
        encodeValue = (${valueEncodingSource}) nixpkgs.lib;
      };
    in { inherit (jobs) __crystalForgeConfigIndex; }
  '';

  partialDefinitionValuesExpression = ''
    let
      flakeRef = "path:${fixture}";
      configurationName = "partial";
      flake = builtins.getFlake flakeRef;
      configuration = builtins.getAttr configurationName flake.nixosConfigurations;
      targetKey = builtins.hashString "sha256" (builtins.toJSON [ flakeRef configurationName ]);
      valueEncoder = (${valueEncodingSource});
      inspector = (${inspectorSource}) {
        inherit flake configuration targetKey;
        provenanceLib = (${provenanceLibSource});
        encodeValue = valueEncoder configuration.pkgs.lib;
      };
      allowedOptionKeys = map (entry: entry.key)
        inspector.__crystalForgeConfigIndex.meta.crystalForgeInspector.options;
      allowedOptionPaths = map (entry: entry.path)
        inspector.__crystalForgeConfigIndex.meta.crystalForgeInspector.options;
    in (${definitionValuesSource}) {
      inherit flake configuration targetKey allowedOptionKeys allowedOptionPaths;
      provenanceLib = (${provenanceLibSource});
      encodeValue = valueEncoder configuration.pkgs.lib;
    }
  '';

  diagnosticStressExpression = ''
    let
      flakeRef = "path:${fixture}";
      configurationName = "diagnosticStress";
      flake = builtins.getFlake flakeRef;
      configuration = builtins.getAttr configurationName flake.nixosConfigurations;
      targetKey = builtins.hashString "sha256" (builtins.toJSON [ flakeRef configurationName ]);
      valueEncoder = (${valueEncodingSource});
    in (${inspectorSource}) {
      inherit flake configuration targetKey;
      provenanceLib = (${provenanceLibSource});
      encodeValue = valueEncoder configuration.pkgs.lib;
    }
  '';

  diagnosticStressDefinitionValuesExpression = ''
    let
      flakeRef = "path:${fixture}";
      configurationName = "diagnosticStress";
      flake = builtins.getFlake flakeRef;
      configuration = builtins.getAttr configurationName flake.nixosConfigurations;
      targetKey = builtins.hashString "sha256" (builtins.toJSON [ flakeRef configurationName ]);
      healthyPath = [ "crystalForgeStress" "zzzHealthy" ];
      allowedOptionPaths = [ healthyPath ];
      allowedOptionKeys = [ (builtins.hashString "sha256" (builtins.toJSON healthyPath)) ];
      valueEncoder = (${valueEncodingSource});
    in (${definitionValuesSource}) {
      inherit flake configuration targetKey allowedOptionKeys allowedOptionPaths;
      provenanceLib = (${provenanceLibSource});
      encodeValue = valueEncoder configuration.pkgs.lib;
    }
  '';

  definitionValuesExpression = ''
    let
      flakeRef = "path:${fixture}";
      configurationName = "good";
      flake = builtins.getFlake flakeRef;
      configuration = builtins.getAttr configurationName flake.nixosConfigurations;
      targetKey = builtins.hashString "sha256" (builtins.toJSON [ flakeRef configurationName ]);
      valueEncoder = (${valueEncodingSource});
    in (${definitionValuesSource}) {
      inherit flake configuration targetKey;
      provenanceLib = (${provenanceLibSource});
      encodeValue = valueEncoder configuration.pkgs.lib;
    }
  '';

  legacyDefinitionValuesExpression = ''
    let
      flakeRef = "path:${legacyFixture}";
      configurationName = "legacy";
      flake = builtins.getFlake flakeRef;
      configuration = builtins.getAttr configurationName flake.nixosConfigurations;
      targetKey = builtins.hashString "sha256" (builtins.toJSON [ flakeRef configurationName ]);
      valueEncoder = (${valueEncodingSource});
    in (${definitionValuesSource}) {
      inherit flake configuration targetKey;
      provenanceLib = (${provenanceLibSource});
      encodeValue = valueEncoder configuration.pkgs.lib;
    }
  '';

  subsetExpression = ''
    let
      jobs = ${expression};
      healthyBefore = builtins.hashString "sha256" (builtins.toJSON [
        "crystalForgeProbe" "healthyBefore"
      ]);
      poison = builtins.hashString "sha256" (builtins.toJSON [
        "crystalForgeProbe" "poison"
      ]);
      healthyAfter = builtins.hashString "sha256" (builtins.toJSON [
        "crystalForgeProbe" "healthyAfter"
      ]);
      names = [
        "__crystalForgeConfigIndex"
        ("meta_" + healthyBefore)
        ("value_" + healthyBefore)
         ("meta_" + poison)
         ("value_" + poison)
         ("meta_" + healthyAfter)
         ("value_" + healthyAfter)
         "__crystalForgeProvenance"
       ];
    in builtins.listToAttrs (map (name: {
      inherit name;
      value = builtins.getAttr name jobs;
    }) names)
  '';

  fullCountExpression = ''
    builtins.toString (builtins.length (builtins.attrNames (${expression})))
  '';
  subsetFile = pkgs.writeText "crystal-forge-config-inspector-subset.nix" subsetExpression;
  fullCountFile = pkgs.writeText "crystal-forge-config-inspector-count.nix" fullCountExpression;
  unsupportedCapabilityFile = pkgs.writeText "crystal-forge-config-inspector-unsupported.nix" unsupportedCapabilityExpression;
  definitionValuesFile = pkgs.writeText "crystal-forge-config-definition-values.nix" definitionValuesExpression;
  legacyDefinitionValuesFile = pkgs.writeText "crystal-forge-config-definition-values-legacy.nix" legacyDefinitionValuesExpression;
  partialFile = pkgs.writeText "crystal-forge-config-inspector-partial.nix" partialExpression;
  rootFailureFile = pkgs.writeText "crystal-forge-config-inspector-root-failure.nix" rootFailureExpression;
  partialDefinitionValuesFile = pkgs.writeText "crystal-forge-config-definition-values-partial.nix" partialDefinitionValuesExpression;
  diagnosticStressFile = pkgs.writeText "crystal-forge-config-inspector-diagnostic-stress.nix" diagnosticStressExpression;
  diagnosticStressDefinitionValuesFile = pkgs.writeText "crystal-forge-config-definition-values-diagnostic-stress.nix" diagnosticStressDefinitionValuesExpression;
in
pkgs.runCommand "crystal-forge-config-inspector-check" {
  nativeBuildInputs = [ pkgs.jq pkgs.nix pkgs.nix-eval-jobs ];
} ''
  export HOME="$TMPDIR"
  export XDG_CACHE_HOME="$TMPDIR/cache"

  nix_args=(--impure --extra-experimental-features 'nix-command flakes')
  count=$(nix eval "''${nix_args[@]}" --raw --expr "import ${fullCountFile}")
  test "$count" -gt 30000
  unsupported=$(nix eval "''${nix_args[@]}" --json --expr "import ${unsupportedCapabilityFile}")
  test "$(printf '%s' "$unsupported" | jq -r '.supported')" = false
  test "$(printf '%s' "$unsupported" | jq -r '.reasonCode')" = helper_capability_unavailable
  ! grep -F 'builtins.elem option.option_key allowedOptionKeys' ${../../packages/default/crates/cf-server/src/models/config_definition_values.nix}
  grep -F 'builtins.hasAttr option.option_key allowedOptionKeySet' ${../../packages/default/crates/cf-server/src/models/config_definition_values.nix} >/dev/null
  ! grep -F 'lib.unique optionKeys' ${../../packages/default/crates/cf-server/src/models/config_inspector.nix}
  ! grep -F "builtins.foldl'" ${../../packages/default/crates/cf-server/src/models/config_inspector.nix}

  nix-eval-jobs \
    --expr "import ${partialFile}" \
    --impure --meta \
    --apply 'derivation: if derivation.meta ? crystalForgeInspector then derivation.meta.crystalForgeInspector else derivation.meta.crystalForgeProvenance' \
    --option experimental-features 'nix-command flakes' \
    --workers 2 > partial.jsonl 2> partial.stderr
  jq -e '
    select(.attr == "__crystalForgeConfigIndex")
    | .error == null
    and .extraValue.optionInventoryComplete == false
    and (.extraValue.optionInventoryDiagnostics | length) == 3
    and ([.extraValue.optionInventoryDiagnostics[] | select(.path == ["crystalForgeProbe", "poison"])] | length == 1)
    and ([.extraValue.optionInventoryDiagnostics[] | select(.path == ["api_token=typed-diagnostic-secret"])] | length == 1)
    and ([.extraValue.optionInventoryDiagnostics[] | select((.path | length) == 1 and (.path[0] | length) == 257)] | length == 1)
    and ([.extraValue.options[] | select(.path == ["crystalForgeProbe", "healthyBefore"])] | length == 1)
    and ([.extraValue.options[] | select(.path == ["crystalForgeProbe", "healthyAfter"])] | length == 1)
    and ([.extraValue.options[] | select(.path == ["crystalForgeProbe", "poison"])] | length == 0)
  ' partial.jsonl >/dev/null
  before_partial_hash=$(nix eval "''${nix_args[@]}" --raw --expr \
    'builtins.hashString "sha256" (builtins.toJSON [ "crystalForgeProbe" "healthyBefore" ])')
  after_partial_hash=$(nix eval "''${nix_args[@]}" --raw --expr \
    'builtins.hashString "sha256" (builtins.toJSON [ "crystalForgeProbe" "healthyAfter" ])')
  poison_partial_hash=$(nix eval "''${nix_args[@]}" --raw --expr \
    'builtins.hashString "sha256" (builtins.toJSON [ "crystalForgeProbe" "poison" ])')
  jq -e --arg attr "meta_$before_partial_hash" 'select(.attr == $attr) | .error == null' partial.jsonl >/dev/null
  jq -e --arg attr "value_$before_partial_hash" 'select(.attr == $attr) | .error == null' partial.jsonl >/dev/null
  jq -e --arg attr "meta_$after_partial_hash" 'select(.attr == $attr) | .error == null' partial.jsonl >/dev/null
  jq -e --arg attr "value_$after_partial_hash" 'select(.attr == $attr) | .error == null' partial.jsonl >/dev/null
  ! jq -e --arg meta "meta_$poison_partial_hash" --arg value "value_$poison_partial_hash" \
    'select(.attr == $meta or .attr == $value)' partial.jsonl >/dev/null
  test "$(jq -r 'select(.attr == "__crystalForgeConfigIndex") | .drvPath' partial.jsonl)" = \
    "$(jq -r 'select(.attr == "meta_'"$before_partial_hash"'") | .drvPath' partial.jsonl)"

  nix-eval-jobs \
    --expr "import ${partialDefinitionValuesFile}" \
    --impure --meta \
    --apply 'derivation: derivation.meta.crystalForgeDefinitionValues' \
    --option experimental-features 'nix-command flakes' \
    --workers 2 > partial-definition-values.jsonl 2> partial-definition-values.stderr
  jq -e 'select(.attr == "__crystalForgeDefinitionIndex") | .error == null and .extraValue.supported == true' \
    partial-definition-values.jsonl >/dev/null
  jq -e --arg prefix "def_value_''${before_partial_hash}_" \
    'select(.attr | startswith($prefix)) | .error == null' partial-definition-values.jsonl >/dev/null
  jq -e --arg prefix "def_value_''${after_partial_hash}_" \
    'select(.attr | startswith($prefix)) | .error == null and .extraValue.value.value == "after"' partial-definition-values.jsonl >/dev/null
  ! jq -e --arg prefix "def_value_''${poison_partial_hash}_" \
    'select(.attr | startswith($prefix))' partial-definition-values.jsonl >/dev/null
  test "$(jq -r 'select(.attr == "__crystalForgeConfigIndex") | .drvPath' partial.jsonl)" = \
    "$(jq -r 'select(.attr == "__crystalForgeDefinitionIndex") | .drvPath' partial-definition-values.jsonl)"
  test "$(jq -r 'select(.attr == "__crystalForgeProvenance") | .extraValue.provenanceDigest' partial.jsonl)" = \
    "$(jq -r 'select(.attr == "__crystalForgeDefinitionIndex") | .extraValue.provenanceDigest' partial-definition-values.jsonl)"

  nix-eval-jobs \
    --expr "import ${diagnosticStressFile}" \
    --impure --meta \
    --apply 'derivation: if derivation.meta ? crystalForgeInspector then derivation.meta.crystalForgeInspector else derivation.meta.crystalForgeProvenance' \
    --option experimental-features 'nix-command flakes' \
    --workers 2 > diagnostic-stress.jsonl 2> diagnostic-stress.stderr
  stress_healthy_hash=$(nix eval "''${nix_args[@]}" --raw --expr \
    'builtins.hashString "sha256" (builtins.toJSON [ "crystalForgeStress" "zzzHealthy" ])')
  jq -e --arg healthy "$stress_healthy_hash" '
    select(.attr == "__crystalForgeConfigIndex")
    | .error == null
    and .extraValue.optionInventoryComplete == false
    and .extraValue.optionInventoryDiagnosticsTruncated == true
    and (.extraValue.optionInventoryDiagnostics | length) == 128
    and (.extraValue.optionInventoryDiagnostics[0].path == ["crystalForgeStress", "poison000"])
    and (.extraValue.optionInventoryDiagnostics[127].path == ["crystalForgeStress", "poison127"])
    and ([.extraValue.options[] | select(.key == $healthy and .path == ["crystalForgeStress", "zzzHealthy"])] | length == 1)
    and ([.extraValue.options[] | select(.path[0] == "crystalForgeStress" and (.path[1] | startswith("poison")))] | length == 0)
  ' diagnostic-stress.jsonl >/dev/null
  jq -e --arg attr "meta_$stress_healthy_hash" 'select(.attr == $attr) | .error == null' diagnostic-stress.jsonl >/dev/null
  jq -e --arg attr "value_$stress_healthy_hash" 'select(.attr == $attr) | .error == null and .extraValue.value.value == "healthy-after-budget"' diagnostic-stress.jsonl >/dev/null
  ! jq -e 'select(.attr | test("^(meta|value)_.*poison"))' diagnostic-stress.jsonl >/dev/null
  jq -e 'select(.attr == "__crystalForgeProvenance") | .error == null and (.extraValue.definitionsByOption | length) == 1' diagnostic-stress.jsonl >/dev/null

  nix-eval-jobs \
    --expr "import ${diagnosticStressDefinitionValuesFile}" \
    --impure --meta \
    --apply 'derivation: derivation.meta.crystalForgeDefinitionValues' \
    --option experimental-features 'nix-command flakes' \
    --workers 1 > diagnostic-stress-definition-values.jsonl 2> diagnostic-stress-definition-values.stderr
  jq -e 'select(.attr == "__crystalForgeDefinitionIndex") | .error == null and .extraValue.definitionCount == 1' diagnostic-stress-definition-values.jsonl >/dev/null
  jq -e --arg prefix "def_value_''${stress_healthy_hash}_" 'select(.attr | startswith($prefix)) | .error == null' diagnostic-stress-definition-values.jsonl >/dev/null
  test "$(wc -l < diagnostic-stress-definition-values.jsonl)" -eq 2
  test "$(jq -r 'select(.attr == "__crystalForgeProvenance") | .extraValue.provenanceDigest' diagnostic-stress.jsonl)" = \
    "$(jq -r 'select(.attr == "__crystalForgeDefinitionIndex") | .extraValue.provenanceDigest' diagnostic-stress-definition-values.jsonl)"

  nix-eval-jobs \
    --expr "import ${rootFailureFile}" \
    --impure --meta \
    --apply 'derivation: derivation.meta.crystalForgeInspector' \
    --option experimental-features 'nix-command flakes' \
    --workers 1 > root-failure.jsonl 2> root-failure.stderr || true
  test "$(wc -l < root-failure.jsonl)" -eq 1
  jq -e '.attr == "__crystalForgeConfigIndex" and .error != null and .drvPath == null' \
    root-failure.jsonl >/dev/null
  target_key=$(nix eval "''${nix_args[@]}" --raw --expr \
    'builtins.hashString "sha256" (builtins.toJSON [ "path:${fixture}" "good" ])')

  nix-eval-jobs \
    --expr "import ${definitionValuesFile}" \
    --impure \
    --meta \
    --apply 'derivation: if derivation.meta ? crystalForgeDefinitionValues then derivation.meta.crystalForgeDefinitionValues else derivation.meta' \
    --option experimental-features 'nix-command flakes' \
    --workers 2 > definition-values.jsonl 2> definition-values.stderr || {
      cat definition-values.stderr >&2
      exit 1
    }

  test "$(wc -l < definition-values.jsonl)" -eq 3068
  test "$(jq -c 'select(.attr == "__crystalForgeDefinitionIndex")' definition-values.jsonl | wc -l)" -eq 1
  jq -e '
    select(.attr == "__crystalForgeDefinitionIndex")
    | .error == null
    and .extraValue.kind == "definition_index"
    and .extraValue.supported == true
    and .extraValue.definitionCount == 3067
    and ([.extraValue.definitions[]] | length == 3067)
  ' definition-values.jsonl >/dev/null
  test "$(jq -r 'select(.attr == "__crystalForgeDefinitionIndex") | .extraValue.targetKey' definition-values.jsonl)" = "$target_key"
  stage2_source_out_path=$(jq -r 'select(.attr == "__crystalForgeDefinitionIndex") | .extraValue.sourceOutPath' definition-values.jsonl)
  test "$(jq -c 'select(.attr | startswith("def_value_"))' definition-values.jsonl | wc -l)" -eq 3067
  test "$(jq -c 'select(.error != null)' definition-values.jsonl | wc -l)" -ge 2
  jq -e 'select(.attr | startswith("def_value_")) | .attr' definition-values.jsonl \
    | sort | uniq -d | test "$(wc -l)" -eq 0
  test "$(jq -s '[.[] | select(.error == null and .drvPath != null) | .drvPath] | unique | length' definition-values.jsonl)" -eq 1

  # COMPATIBILITY: Nixpkgs 25.05 returns a list from collectModules and does
  # not expose the graph required to prove replay equality. Stage 2 must report
  # unsupported provenance instead of failing the complete inspection.
  nix-eval-jobs \
    --expr "import ${legacyDefinitionValuesFile}" \
    --impure \
    --meta \
    --apply 'derivation: if derivation.meta ? crystalForgeDefinitionValues then derivation.meta.crystalForgeDefinitionValues else derivation.meta' \
    --option experimental-features 'nix-command flakes' \
    --workers 1 > legacy-definition-values.jsonl 2> legacy-definition-values.stderr || {
      cat legacy-definition-values.stderr >&2
      exit 1
    }

  test "$(wc -l < legacy-definition-values.jsonl)" -eq 1
  jq -e '
    .attr == "__crystalForgeDefinitionIndex"
    and .error == null
    and .extraValue.kind == "definition_index"
    and .extraValue.supported == false
    and .extraValue.reasonCode == "helper_capability_unavailable"
    and .extraValue.definitionCount == 0
    and .extraValue.definitions == []
  ' legacy-definition-values.jsonl >/dev/null
  ! grep -F 'expected a set but found a list' legacy-definition-values.stderr

  before_hash=$(nix eval "''${nix_args[@]}" --raw --expr \
    'builtins.hashString "sha256" (builtins.toJSON [ "crystalForgeProbe" "healthyBefore" ])')
  poison_hash=$(nix eval "''${nix_args[@]}" --raw --expr \
    'builtins.hashString "sha256" (builtins.toJSON [ "crystalForgeProbe" "poison" ])')
  after_hash=$(nix eval "''${nix_args[@]}" --raw --expr \
    'builtins.hashString "sha256" (builtins.toJSON [ "crystalForgeProbe" "healthyAfter" ])')

  nix-eval-jobs \
    --expr "import ${subsetFile}" \
    --impure \
    --meta \
     --apply 'derivation: if derivation.meta ? crystalForgeInspector then derivation.meta.crystalForgeInspector else derivation.meta.crystalForgeProvenance' \
    --option experimental-features 'nix-command flakes' \
    --workers 2 > result.jsonl 2> result.stderr

   test "$(wc -l < result.jsonl)" -eq 8
  test "$(jq -c 'select(.error != null)' result.jsonl | wc -l)" -ge 1
  test "$(jq -c 'select(.drvPath != null) | .drvPath' result.jsonl | sort -u | wc -l)" -eq 1
  test "$(jq -r 'select(.attr == "__crystalForgeConfigIndex") | .extraValue.targetKey' result.jsonl)" = "$target_key"
  stage1_drv_path=$(jq -r 'select(.attr == "__crystalForgeConfigIndex") | .drvPath' result.jsonl)
  stage1_source_out_path=$(jq -r 'select(.attr == "__crystalForgeConfigIndex") | .extraValue.sourceOutPath' result.jsonl)
  test "$stage1_source_out_path" = "$stage2_source_out_path"
  stage2_drv_path=$(jq -r 'select(.attr == "__crystalForgeDefinitionIndex") | .drvPath' definition-values.jsonl)
  test "$stage1_drv_path" = "$stage2_drv_path"

  jq -e '
    select(.attr == "__crystalForgeConfigIndex")
    | .error == null
    and .extraValue.kind == "index"
    and ([.extraValue.options[] | select(.path == ["crystalForgeProbe", "healthyBefore"])] | length == 1)
  ' result.jsonl >/dev/null

  jq -e --arg attr "meta_$before_hash" \
    'select(.attr == $attr) | .error == null and .extraValue.kind == "metadata"' \
    result.jsonl >/dev/null

  jq -e --arg attr "value_$before_hash" \
    'select(.attr == $attr) | .error == null and .extraValue.kind == "value"' \
    result.jsonl >/dev/null

  jq -e --arg attr "value_$poison_hash" \
    'select(.attr == $attr) | (.error // "") | contains("selected poisoned option forced")' \
    result.jsonl >/dev/null

  jq -e --arg attr "meta_$poison_hash" \
    'select(.attr == $attr) | .error == null and .extraValue.kind == "metadata"' \
    result.jsonl >/dev/null

  jq -e --arg attr "meta_$after_hash" \
    'select(.attr == $attr) | .error == null and .extraValue.kind == "metadata"' \
    result.jsonl >/dev/null

   jq -e --arg attr "value_$after_hash" \
     'select(.attr == $attr) | .error == null and .extraValue.kind == "value"' \
     result.jsonl >/dev/null

    jq -e '
      select(.attr == "__crystalForgeProvenance")
      | .error == null
     and .extraValue.adapterVersion == 1
     and .extraValue.supported == true
     and (.extraValue.targetLibVersion | type == "string")
     and (.extraValue.targetModuleSystemPath | type == "string")
     and .extraValue.graphReplay.equal == true
     and .extraValue.graphReplay.actualNodeCount == .extraValue.graphReplay.replayNodeCount
     and .extraValue.seedEvidence.hiddenPkgsModulePresent == true
     and .extraValue.seedEvidence.hiddenModulesModulePresent == true
     and ([.extraValue.seedEvidence.modulesLocationSources[] | select(contains("explicit-modules-location.nix"))] | length == 1)
   ' result.jsonl >/dev/null

   jq -e '
     select(.attr == "__crystalForgeProvenance")
     | [.extraValue.definitionsByOption[] | select(.path == ["crystalForgeProbe", "poison"]) | .definitions[]]
     | any(.status == "priority_discarded")
   ' result.jsonl >/dev/null

   jq -e '
     select(.attr == "__crystalForgeProvenance")
     | [.extraValue.definitionsByOption[] | select(.path == ["crystalForgeProbe", "samePriority"]) | .definitions[]]
     | map(select(.status == "active_surviving")) | length == 2
   ' result.jsonl >/dev/null

   jq -e '
     select(.attr == "__crystalForgeProvenance")
     | [.extraValue.definitionsByOption[] | select(.path == ["crystalForgeProbe", "conditionalFalse"]) | .definitions[]]
     | length == 0
   ' result.jsonl >/dev/null

   jq -e '
     select(.attr == "__crystalForgeProvenance")
     | [.extraValue.definitionsByOption[] | select(.path == ["crystalForgeProbe", "conditionalTrue"]) | .definitions[]]
     | any(.status == "active_surviving")
   ' result.jsonl >/dev/null

   jq -e '
     select(.attr == "__crystalForgeProvenance")
     | [.extraValue.definitionsByOption[] | select(.path == ["crystalForgeProbe", "imported"]) | .definitions[]]
     | length == 1
   ' result.jsonl >/dev/null

   jq -e '
     select(.attr == "__crystalForgeProvenance")
     | [.extraValue.definitionsByOption[] | select(.path == ["crystalForgeProbe", "disabled"]) | .definitions[]]
     | length == 0
   ' result.jsonl >/dev/null

   jq -e '
     select(.attr == "__crystalForgeProvenance")
     | [.extraValue.definitionsByOption[] | select(.path == ["crystalForgeProbe", "ordered"]) | .definitions[]]
     | all(.status == "active_surviving")
     and map(.surviving_merge_order) == [1, 0]
   ' result.jsonl >/dev/null

    jq -e '
      select(.attr == "__crystalForgeProvenance")
      | [.extraValue.definitionsByOption[] | select(.path == ["crystalForgeProbe", "merged"]) | .definitions[]]
      | length == 2 and all(.status == "active_surviving")
    ' result.jsonl >/dev/null

    jq -e '
      select(.attr == "__crystalForgeProvenance")
      | [.extraValue.definitionsByOption[] | select(.path == ["crystalForgeProbe", "priorityProbe"]) | .definitions[]]
      | map(.priority) | sort == [50, 100, 500, 1000]
    ' result.jsonl >/dev/null

   ! grep -E 'unrelated configuration forced|namespace-change-me' result.stderr
  touch "$out"
''
