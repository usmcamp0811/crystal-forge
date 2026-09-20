{ pkgs, ... }:

let
  observerSource = builtins.readFile ../../packages/default/crates/cf-server/src/models/config_observer.nix;
  encoderSource = builtins.readFile ../../packages/default/crates/cf-server/src/models/config_value_encoding.nix;
  shallowObserverSource = builtins.readFile ../../packages/default/crates/cf-server/src/models/config_shallow_observer.nix;
  fixture = pkgs.runCommand "crystal-forge-config-observer-fixture" { } ''
    mkdir -p "$out"
    cat > "$out/flake.nix" <<'EOF'
    {
      inputs.nixpkgs.url = "path:${pkgs.path}";
      outputs = { nixpkgs, ... }:
        let
          lib = nixpkgs.lib;
          evaluated = lib.nixosSystem {
            # Pinned rather than `builtins.currentSystem` so the same fixture
            # is usable under `--option pure-eval true`, which rejects
            # impure builtins.
            system = "${pkgs.stdenv.hostPlatform.system}";
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
      childOffset = 0;
      encodeValue = _: _: _: throw "value encoder was forced";
      shallowObserver = (${shallowObserverSource});
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
      childOffset = 0;
      encodeValue = _: _: value: { kind = "scalar"; inherit value; };
      shallowObserver = (${shallowObserverSource});
    }
  '';
  poisonOptionFile = pkgs.writeText "config-observer-poison-option.nix" ''
    let
      flake = builtins.getFlake "path:${fixture}";
      configuration = flake.nixosConfigurations.test;
      encodeValue = (${encoderSource}) configuration.pkgs.lib;
    in (${observerSource}) {
      inherit flake configuration encodeValue;
      targetKey = "test";
      operation = "option";
      path = [ "crystalForgeConfigured" "nestedPoison" ];
      childOffset = 0;
      shallowObserver = (${shallowObserverSource});
    }
  '';
  servicesPageFile = offset: pkgs.writeText "config-observer-services-${toString offset}.nix" ''
    let
      flake = builtins.getFlake "path:${fixture}";
      configuration = flake.nixosConfigurations.test;
    in (${observerSource}) {
      inherit flake configuration;
      targetKey = "test";
      operation = "prefix";
      path = [ "services" ];
      childOffset = ${toString offset};
      encodeValue = _: _: _: throw "value encoder was forced";
      shallowObserver = (${shallowObserverSource});
    }
  '';
  servicesFirstFile = servicesPageFile 0;
  servicesSecondFile = servicesPageFile 512;
  provenanceFile = pkgs.writeText "config-observer-provenance.nix"
    (expressionFor "provenance" [ "crystalForgeProvenance" "many" ]);
  # A separately locked fixture for the pure-evaluation regression.
  #
  # `--option pure-eval true` rejects unlocked flake inputs and absolute path
  # literals, so this fixture carries a real `flake.lock` exactly as a
  # production flake does. The unlocked `fixture` above is deliberately left
  # alone so the existing "observer never writes a lock file" assertions keep
  # their meaning.
  lockedFixture = pkgs.runCommand "crystal-forge-config-observer-locked-fixture" {
    nativeBuildInputs = [ pkgs.jq pkgs.nix ];
  } ''
    mkdir -p "$out"
    cat > "$out/flake.nix" <<'EOF'
    {
      inputs.nixpkgs.url = "path:${pkgs.path}";
      outputs = { nixpkgs, ... }:
        let lib = nixpkgs.lib; in {
          nixosConfigurations.test = lib.nixosSystem {
            system = "${pkgs.stdenv.hostPlatform.system}";
            modules = [
              ({ lib, ... }: {
                options.crystalForgeConfigured = {
                  defaultOnly = lib.mkOption { type = lib.types.str; default = "default"; };
                  ordinary = lib.mkOption { type = lib.types.str; default = "default"; };
                  mkForce = lib.mkOption { type = lib.types.str; default = "default"; };
                  losing = lib.mkOption { type = lib.types.str; default = "default"; };
                };
                config = {
                  crystalForgeConfigured = {
                    ordinary = "ordinary";
                    mkForce = lib.mkForce "forced";
                    losing = lib.mkOverride 2000 "loser";
                  };
                  boot.isContainer = true;
                  # Keep the carrier closure small and offline-evaluable so
                  # the sandboxed check never needs a substituter.
                  networking.hostName = "config-observer-fixture";
                  networking.domain = "test";
                  documentation.enable = false;
                  documentation.nixos.enable = false;
                  system.stateVersion = "26.05";
                };
              })
            ];
          };
        };
    }
    EOF
    # The locked narHash must equal the real NAR hash of the input path.
    nixpkgsHash="$(nix --extra-experimental-features nix-command \
      hash path --type sha256 --sri "${pkgs.path}")"
    jq -n --arg p "${pkgs.path}" --arg h "$nixpkgsHash" '{
      nodes: {
        nixpkgs: {
          locked: { lastModified: 1, narHash: $h, path: $p, type: "path" },
          original: { path: $p, type: "path" }
        },
        root: { inputs: { nixpkgs: "nixpkgs" } }
      },
      root: "root",
      version: 7
    }' > "$out/flake.lock"
  '';
  # Mirrors `build_observer_expression` in
  # `packages/default/crates/cf-server/src/services/config_observations.rs`.
  # The selection is inline structured Nix, never `builtins.readFile` of a
  # private temporary path, because the production command enables
  # `--option pure-eval true`. `@FLAKEREF@` is substituted at build time with
  # the narHash-qualified store flake reference production uses.
  pureEvalTemplate = pkgs.writeText "config-observer-pure-eval-template.nix" ''
    let
      flake = builtins.getFlake "@FLAKEREF@";
      configuration = builtins.getAttr "test" flake.nixosConfigurations;
      encodeValue = (${encoderSource}) configuration.pkgs.lib;
    in (${observerSource}) {
      inherit flake configuration encodeValue;
      targetKey = builtins.hashString "sha256" (builtins.toJSON [ "@FLAKEREF@" "test" configuration.config.system.build.toplevel.drvPath ]);
      operation = "configured_index";
      path = [ ];
      childOffset = 0;
      shallowObserver = (${shallowObserverSource});
    }
  '';
in
pkgs.runCommand "crystal-forge-config-observer-check" {
  nativeBuildInputs = [ pkgs.jq pkgs.nix-eval-jobs pkgs.nix ];
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
  unconfigured_path() {
    jq -s -e --arg name "$1" '
      [ .[]
        | select(.attr | startswith("configured_"))
        | select(.extraValue.path_components == ["crystalForgeConfigured", $name])
      ]
      | length == 1
        and .[0].error == null
        and .[0].extraValue.configured == false
    ' configured.jsonl >/dev/null
  }
  unconfigured_path defaultOnly
  configured_path ordinary
  configured_path mkDefault
  configured_path mkForce
  configured_path generated
  unconfigured_path losing
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

  nix-eval-jobs --expr "import ${poisonOptionFile}" "''${args[@]}" --workers 1 \
    > poison-option.jsonl 2> poison-option.stderr
  jq -e '
    .attr == "observation" and .error == null
    and .extraValue.path_components == ["crystalForgeConfigured", "nestedPoison"]
    and .extraValue.declared_type == "attrs"
    and .extraValue.is_defined == true
    and .extraValue.highest_prio == 100
    and .extraValue.value == {
      "kind":"failed",
      "value":{"code":"value_unavailable","message":"Option value is unavailable"}
    }
  ' poison-option.jsonl >/dev/null
  ! grep -F 'nested option.value was forced' poison-option.jsonl poison-option.stderr

  nix-eval-jobs --expr "import ${servicesFirstFile}" "''${args[@]}" --workers 1 \
    > services-first.jsonl 2> services-first.stderr
  nix-eval-jobs --expr "import ${servicesSecondFile}" "''${args[@]}" --workers 1 \
    > services-second.jsonl 2> services-second.stderr
  jq -e '
    .attr == "observation" and .error == null
    and .extraValue.child_offset == 0
    and .extraValue.total_children > 512
    and .extraValue.children_truncated == true
    and (.extraValue.children | length) == 512
  ' services-first.jsonl >/dev/null
  jq -e '
    .attr == "observation" and .error == null
    and .extraValue.child_offset == 512
    and .extraValue.total_children > 512
    and (.extraValue.children | length) > 0
    and .extraValue.children[0].path_components
      > (input.extraValue.children[-1].path_components)
  ' services-second.jsonl services-first.jsonl >/dev/null
  test ! -e "${fixture}/flake.lock"

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

  # ── Production command path under pure evaluation ────────────────────────
  # Regression for the observed production failure: the configured index was
  # constructed with `builtins.readFile` of a private temporary selection
  # file while `--option pure-eval true` was enabled, so `nix-eval-jobs`
  # exited 0 and emitted an error record instead of an index.
  narHash="$(nix --extra-experimental-features nix-command hash path --type sha256 --sri "${lockedFixture}")"
  encodedHash="$(jq -rn --arg hash "$narHash" '$hash | @uri')"
  flakeref="path:${lockedFixture}?narHash=$encodedHash"
  sed "s|@FLAKEREF@|$flakeref|g" "${pureEvalTemplate}" > pure-eval.nix

  # These flags are the exact set emitted by `build_observer_command`.
  nix-eval-jobs --expr "$(cat pure-eval.nix)" \
    --option pure-eval true \
    --meta --apply 'derivation: derivation.meta.crystalForgeConfigObservation' \
    --option experimental-features 'nix-command flakes' \
    --workers 2 \
    > pure-configured.jsonl 2> pure-configured.stderr || true

  # The required index record must exist and must not be an error record.
  # Exit status alone is not evidence: a failed job still exits 0.
  if ! jq -e 'select(.attr == "__crystalForgeConfiguredIndex") | .error == null' \
      pure-configured.jsonl >/dev/null; then
    echo "pure-eval configured index did not produce a successful index record" >&2
    echo "--- stdout ---" >&2; cat pure-configured.jsonl >&2
    echo "--- stderr ---" >&2; tail -c 4000 pure-configured.stderr >&2
    exit 1
  fi
  ! grep -Fq 'forbidden in pure evaluation mode' pure-configured.jsonl
  ! grep -Fq 'forbidden in pure evaluation mode' pure-configured.stderr

  pure_configured_path() {
    jq -e --arg name "$1" '
      select(.attr | startswith("configured_"))
      | select(.error == null)
      | select(.extraValue.path_components == ["crystalForgeConfigured", $name])
      | .extraValue.configured == true
    ' pure-configured.jsonl >/dev/null
  }
  pure_unconfigured_path() {
    jq -s -e --arg name "$1" '
      [ .[]
        | select(.attr | startswith("configured_"))
        | select(.extraValue.path_components == ["crystalForgeConfigured", $name])
      ]
      | length == 1
        and .[0].error == null
        and .[0].extraValue.configured == false
    ' pure-configured.jsonl >/dev/null
  }
  # A real configured assignment is classified; a declaration-only default is
  # not. Identities are asserted, not merely the process exit status.
  pure_configured_path ordinary
  pure_configured_path mkForce
  pure_unconfigured_path defaultOnly
  pure_unconfigured_path losing

  # Every emitted job must carry the single shared carrier identity that
  # reconciliation binds to the request.
  test "$(jq -r 'select(.error == null) | .drvPath' pure-configured.jsonl | sort -u | wc -l)" -eq 1
  test ! -e "${fixture}/flake.lock"

  touch "$out"
''
