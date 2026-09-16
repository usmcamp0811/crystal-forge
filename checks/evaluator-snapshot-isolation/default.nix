{ pkgs, ... }:

# Verifies that the primary evaluator retains the pre-TASK-440 failure boundary.
# The fixture includes lazy metadata that fails when inspected. The primary
# expression must evaluate only the system derivation and policy metadata.

let
  fixtureSource = pkgs.writeText "flake.nix" ''
    {
      outputs = { self }:
        let
          system = (builtins.derivation {
            name = "crystal-forge-primary-evaluator-chesty";
            system = "x86_64-linux";
            builder = "/bin/sh";
            args = [ "-c" "touch $out" ];
          }) // {
            meta = builtins.abort "original system meta must remain lazy";
          };
        in {
          nixosConfigurations.chesty = {
            config = {
              system.build.toplevel = system;
              systemd.services.crystal-forge-agent.enable = true;
              services.crystal-forge = {
                enable = false;
                client.enable = false;
              };
              crystalForgePolicyMarker = true;
            };

            # These values reproduce the two lazy failure classes. Neither is
            # part of the primary system or policy evaluation contract.
            options = builtins.abort "options must remain lazy";
            unrelatedModuleMetadata = builtins.abort "module metadata must remain lazy";
            _module.graph = builtins.abort "module graph must remain lazy";
          };

          # The scoped primary evaluator must filter this declaration before
          # nix-eval-jobs forces the configuration value.
          nixosConfigurations.unmanaged =
            builtins.abort "unmanaged configuration must not be evaluated";

          nixosModules.broken = { lib, ... }:
            with lib.namespace-change-me;
            { namespace-change-me.enable = true; };
        };
    }
  '';
  fixture = pkgs.runCommand "primary-evaluator-fixture" { } ''
    mkdir "$out"
    cp ${fixtureSource} "$out/flake.nix"
    printf '%s\n' '{"nodes":{"root":{"inputs":{}}},"root":"root","version":7}' \
      > "$out/flake.lock"
  '';

  primaryExpression =
    ../../packages/default/crates/cf-server/src/models/primary_evaluation.nix;
  expression = ''
    (${builtins.readFile primaryExpression}) {
      flakeRef = "__LOCKED_FIXTURE_REF__";
      requestedRevision = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
      configurationNames = [ "chesty" ];
      policyCheckers.chesty = config: {
        architectureGate = config.crystalForgePolicyMarker;
      };
    }
  '';
in
pkgs.runCommand "crystal-forge-primary-evaluator-isolation-check" {
  nativeBuildInputs = [ pkgs.jq pkgs.nix-eval-jobs.nix pkgs.nix-eval-jobs ];
} ''
  export HOME="$TMPDIR"
  export XDG_CACHE_HOME="$TMPDIR/cache"
  export NIX_CONFIG='experimental-features = nix-command flakes
  pure-eval = false'
  fixture_nar_hash="$(nix --extra-experimental-features nix-command \
    hash path --type sha256 --sri ${fixture})"
  encoded_nar_hash="$(jq -rn --arg value "$fixture_nar_hash" '$value | @uri')"
  locked_fixture_ref="path:${fixture}?narHash=$encoded_nar_hash"
  expression='${expression}'
  expression="''${expression/__LOCKED_FIXTURE_REF__/$locked_fixture_ref}"
  nix-eval-jobs \
    --expr "$expression" \
    --meta \
    --apply 'derivation: derivation.meta.policies' \
    --option experimental-features 'nix-command flakes' \
    --option pure-eval true \
    --option allow-import-from-derivation true \
    --workers 1 \
    --max-memory-size 0 > result.jsonl

  test "$(wc -l < result.jsonl)" -eq 1
  jq -e '
    .attrPath == ["chesty"] and
    (.drvPath | type == "string" and endswith(".drv")) and
    (.error == null) and
    (.extraValue.architectureGate == true) and
    (.extraValue.cfAgentEnabled == true) and
    (.extraValue.requestedSourceRevision ==
      "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
  ' result.jsonl >/dev/null

  touch "$out"
''
