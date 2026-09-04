{ pkgs, ... }:

# Verifies that the primary evaluator retains the pre-TASK-440 failure boundary.
# The fixture includes lazy metadata that fails when inspected. The primary
# expression must evaluate only the system derivation and policy metadata.

let
  fixture = pkgs.writeTextDir "flake.nix" ''
    {
      inputs.nixpkgs.url = "path:${pkgs.path}";

      outputs = { self, nixpkgs }:
        let
          lib = nixpkgs.lib;
          duplicateOptions = (lib.evalModules {
            modules = [
              ({ lib, ... }: {
                options.crystal-forge.stig.active = lib.mkOption {
                  type = lib.types.bool;
                  default = false;
                };
              })
              ({ lib, ... }: {
                options.crystal-forge.stig.active = lib.mkOption {
                  type = lib.types.bool;
                  default = false;
                };
              })
            ];
          }).options;
          missingNamespaceModule =
            ({ lib, ... }:
              with lib.namespace-change-me;
              { namespace-change-me.enable = true; })
            { inherit lib; };
          system =
            (nixpkgs.legacyPackages.''${builtins.currentSystem}.runCommand
              "crystal-forge-primary-evaluator-chesty"
              { }
              "touch $out") // {
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
            options = duplicateOptions;
            unrelatedModuleMetadata = missingNamespaceModule;
            _module.graph = builtins.abort "module graph must remain lazy";
          };

          nixosModules.broken = { lib, ... }:
            with lib.namespace-change-me;
            { namespace-change-me.enable = true; };
        };
    }
  '';

  primaryExpression =
    ../../packages/default/crates/cf-server/src/models/primary_evaluation.nix;
  expression = ''
    (import ${primaryExpression}) {
      flakeRef = "path:${fixture}";
      requestedRevision = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
      policyCheckers.chesty = config: {
        architectureGate = config.crystalForgePolicyMarker;
      };
    }
  '';
in
pkgs.runCommand "crystal-forge-primary-evaluator-isolation-check" {
  nativeBuildInputs = [ pkgs.jq pkgs.nix pkgs.nix-eval-jobs ];
} ''
  export HOME="$TMPDIR"
  export XDG_CACHE_HOME="$TMPDIR/cache"
  nix-eval-jobs \
    --expr '${expression}' \
    --impure \
    --meta \
    --apply 'derivation: derivation.meta.policies' \
    --option experimental-features 'nix-command flakes' \
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
