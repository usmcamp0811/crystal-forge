{ pkgs, ... }:

let
  inspectorSource = builtins.readFile ../../packages/default/crates/cf-server/src/models/config_inspector.nix;
  fixture = pkgs.runCommand "crystal-forge-config-inspector-fixture" { } ''
    mkdir -p "$out"
    cat > "$out/flake.nix" <<'EOF'
    {
      inputs.base.url = "path:${pkgs.path}";

      outputs = { self, base, ... }:
        let
          lib = base.lib;
          module = { lib, pkgs, ... }: {
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
            };

            config = {
              crystalForgeProbe.healthyBefore = "before";
              crystalForgeProbe.healthyAfter = "after";
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
            modules = [ module ];
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

  expression = ''
    (${inspectorSource}) {
      flakeRef = "path:${fixture}";
      configurationName = "good";
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
in
pkgs.runCommand "crystal-forge-config-inspector-check" {
  nativeBuildInputs = [ pkgs.jq pkgs.nix pkgs.nix-eval-jobs ];
} ''
  export HOME="$TMPDIR"
  export XDG_CACHE_HOME="$TMPDIR/cache"

  nix_args=(--impure --extra-experimental-features 'nix-command flakes')
  count=$(nix eval "''${nix_args[@]}" --raw --expr "import ${fullCountFile}")
  test "$count" -gt 30000
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
    --apply 'derivation: derivation.meta.crystalForgeInspector' \
    --option experimental-features 'nix-command flakes' \
    --workers 2 > result.jsonl 2> result.stderr

  test "$(wc -l < result.jsonl)" -eq 7
  test "$(jq -c 'select(.error != null)' result.jsonl | wc -l)" -ge 1
  test "$(jq -c 'select(.drvPath != null) | .drvPath' result.jsonl | sort -u | wc -l)" -eq 1

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

  ! grep -E 'unrelated configuration forced|namespace-change-me' result.stderr
  touch "$out"
''
