{ pkgs, ... }:

let
  observerSource = builtins.readFile ../../packages/default/crates/cf-server/src/models/config_observer.nix;
  fixture = pkgs.runCommand "crystal-forge-config-observer-fixture" { } ''
    mkdir -p "$out"
    cat > "$out/flake.nix" <<'EOF'
    {
      inputs.nixpkgs.url = "path:${pkgs.path}";
      outputs = { nixpkgs, ... }:
        let
          lib = nixpkgs.lib;
          evaluated = lib.nixosSystem {
            system = builtins.currentSystem;
            modules = [
              ({ lib, ... }: {
                options.crystalForgeConfigured = {
                  defaultOnly = lib.mkOption { type = lib.types.str; default = "default"; };
                  ordinary = lib.mkOption { type = lib.types.str; default = "default"; };
                  mkDefault = lib.mkOption { type = lib.types.str; default = "default"; };
                  mkForce = lib.mkOption { type = lib.types.str; default = "default"; };
                  generated = lib.mkOption { type = lib.types.str; default = "default"; };
                  losing = lib.mkOption { type = lib.types.str; default = "default"; };
                  tie = lib.mkOption { type = lib.types.str; default = "default"; };
                  defaultless = lib.mkOption { type = lib.types.str; };
                  throwingApply = lib.mkOption {
                    type = lib.types.str;
                    apply = _: throw "option.value/apply was forced";
                  };
                  nestedPoison = lib.mkOption { type = lib.types.attrs; };
                };
                options.crystalForgeProvenance.many = lib.mkOption { type = lib.types.str; };
                config = {
                  crystalForgeConfigured = {
                    ordinary = "ordinary";
                    mkDefault = lib.mkDefault "module-default";
                    mkForce = lib.mkForce "forced";
                    generated = lib.mkIf true "generated";
                    losing = lib.mkOverride 2000 "loser";
                    tie = lib.mkOptionDefault "tie";
                    defaultless = lib.mkOptionDefault "defaultless";
                    throwingApply = "poison";
                    nestedPoison = { healthy = "ok"; poison = throw "nested option.value was forced"; };
                  };
                  boot.isContainer = true;
                  system.stateVersion = "26.05";
                };
              })
            ] ++ builtins.genList (_: { lib, ... }: {
              config.crystalForgeProvenance.many = lib.mkDefault "same";
            }) 513;
          };
          ambiguous = evaluated.options.crystalForgeConfigured.tie // {
            definitions = throw "ambiguous classifier poison";
          };
          ordinary = evaluated.options.crystalForgeConfigured.ordinary // {
            definitionsWithLocations = throw "configured classifier accessed definition locations";
          };
        in {
          nixosConfigurations.test = evaluated // {
            options = evaluated.options // {
              crystalForgeConfigured = evaluated.options.crystalForgeConfigured // {
                inherit ambiguous ordinary;
              };
              poisonSibling = throw "root sibling poison";
            };
          };
        };
    }
    EOF
  '';
  expressionFor = operation: path: ''
    let
      flake = builtins.getFlake "path:${fixture}";
      configuration = flake.nixosConfigurations.test;
    in (${observerSource}) {
      inherit flake configuration;
      targetKey = "test";
      operation = ${builtins.toJSON operation};
      path = builtins.fromJSON ${builtins.toJSON (builtins.toJSON path)};
      encodeValue = _: _: _: throw "value encoder was forced";
    }
  '';
  configuredFile = pkgs.writeText "config-observer-configured.nix"
    (expressionFor "configured_index" [ ]);
  rootFile = pkgs.writeText "config-observer-root.nix"
    (expressionFor "root" [ ]);
  prefixFile = pkgs.writeText "config-observer-prefix.nix"
    (expressionFor "prefix" [ "crystalForgeConfigured" ]);
  optionFile = pkgs.writeText "config-observer-option.nix" ''
    let
      flake = builtins.getFlake "path:${fixture}";
      configuration = flake.nixosConfigurations.test;
    in (${observerSource}) {
      inherit flake configuration;
      targetKey = "test";
      operation = "option";
      path = [ "crystalForgeConfigured" "ordinary" ];
      encodeValue = _: _: value: { kind = "scalar"; inherit value; };
    }
  '';
  provenanceFile = pkgs.writeText "config-observer-provenance.nix"
    (expressionFor "provenance" [ "crystalForgeProvenance" "many" ]);
in
pkgs.runCommand "crystal-forge-config-observer-check" {
  nativeBuildInputs = [ pkgs.jq pkgs.nix-eval-jobs ];
} ''
  export HOME="$TMPDIR"
  export XDG_CACHE_HOME="$TMPDIR/cache"
  args=(--impure --meta --apply 'derivation: derivation.meta.crystalForgeConfigObservation' --option experimental-features 'nix-command flakes')

  nix-eval-jobs --expr "import ${configuredFile}" "''${args[@]}" --workers 2 \
    > configured.jsonl 2> configured.stderr || true
  test ! -e "${fixture}/flake.lock"
  jq -e 'select(.attr == "__crystalForgeConfiguredIndex") | .error == null and .extraValue.total_traversed > 16000' \
    configured.jsonl >/dev/null
  configured_path() {
    jq -e --arg name "$1" '
      select(.attr | startswith("configured_"))
      | select(.error == null)
      | select(.extraValue.path_components == ["crystalForgeConfigured", $name])
      | .extraValue.configured == true
    ' configured.jsonl >/dev/null
  }
  absent_path() {
    ! jq -e --arg name "$1" '
      select(.attr | startswith("configured_"))
      | select(.error == null)
      | select(.extraValue.path_components == ["crystalForgeConfigured", $name])
      | .extraValue.configured == true
    ' configured.jsonl >/dev/null
  }
  absent_path defaultOnly
  configured_path ordinary
  configured_path mkDefault
  configured_path mkForce
  configured_path generated
  absent_path losing
  configured_path tie
  configured_path defaultless
  configured_path throwingApply
  configured_path nestedPoison
  test "$(jq -c 'select(.attr | startswith("configured_")) | select((.error // "") | contains("ambiguous classifier poison"))' configured.jsonl | wc -l)" -eq 1
  ! grep -F 'option.value/apply was forced' configured.stderr
  ! grep -F 'nested option.value was forced' configured.stderr
  ! grep -F 'configured classifier accessed definition locations' configured.stderr

  nix-eval-jobs --expr "import ${rootFile}" "''${args[@]}" --workers 1 \
    > root.jsonl 2> root.stderr
  test ! -e "${fixture}/flake.lock"
  jq -e '
    .attr == "observation" and .error == null
    and .extraValue.kind == "root"
    and ([.extraValue.children[] | select(.path_components == ["crystalForgeConfigured"] and .kind == "prefix")] | length == 1)
    and ([.extraValue.children[] | select(.path_components == ["poisonSibling"] and .kind == "unavailable")] | length == 1)
  ' root.jsonl >/dev/null

  nix-eval-jobs --expr "import ${prefixFile}" "''${args[@]}" --workers 1 \
    > prefix.jsonl 2> prefix.stderr
  test ! -e "${fixture}/flake.lock"
  jq -e '
    .attr == "observation" and .error == null
    and .extraValue.kind == "prefix"
    and ([.extraValue.children[] | select(.path_components == ["crystalForgeConfigured", "ordinary"] and .kind == "option")] | length == 1)
  ' prefix.jsonl >/dev/null
  ! grep -F 'option.value/apply was forced' root.stderr prefix.stderr
  ! grep -F 'nested option.value was forced' root.stderr prefix.stderr

  nix-eval-jobs --expr "import ${optionFile}" "''${args[@]}" --workers 1 \
    > option.jsonl 2> option.stderr
  test ! -e "${fixture}/flake.lock"
  jq -e '
    .attr == "observation" and .error == null
    and .extraValue.kind == "option"
    and .extraValue.path_components == ["crystalForgeConfigured", "ordinary"]
    and .extraValue.value == {"kind":"scalar","value":"ordinary"}
  ' option.jsonl >/dev/null

  nix-eval-jobs --expr "import ${provenanceFile}" "''${args[@]}" --workers 1 \
    > provenance.jsonl 2> provenance.stderr
  test ! -e "${fixture}/flake.lock"
  jq -e '
    .attr == "observation" and .error == null
    and .extraValue.kind == "provenance"
    and .extraValue.path_components == ["crystalForgeProvenance", "many"]
    and .extraValue.total_definitions == 513
    and .extraValue.definitions_truncated == true
    and (.extraValue.definitions | length) == 512
    and all(.extraValue.definitions[]; has("source_path") and has("priority"))
  ' provenance.jsonl >/dev/null

  touch "$out"
''
