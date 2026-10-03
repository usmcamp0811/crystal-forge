{
  inputs,
  lib,
  pkgs,
  ...
}: let
  system = pkgs.stdenv.hostPlatform.system;
  evaluatorNix = pkgs.nix-eval-jobs.nix;
  packages = pkgs.crystal-forge.default;
  componentBuilder = packages.cf-builder-drv;
  publicBuilder = packages.builder;
  moduleSystem = inputs.nixpkgs.lib.nixosSystem {
    inherit system;
    modules = [
      inputs.self.nixosModules.crystal-forge
      {
        system.stateVersion = "26.05";
        services.crystal-forge = {
          enable = true;
          server.enable = true;
          client.enable = false;
          build.enable = true;
        };
      }
    ];
  };
  moduleConfig = moduleSystem.config;
in
  # INVARIANT: Both colocated services and the default builder package use the
  # Nix CLI linked to nix-eval-jobs. A package wrapper must not shadow this CLI.
  assert moduleConfig.services.crystal-forge.build.package == componentBuilder;
  assert lib.elem evaluatorNix moduleConfig.systemd.services.crystal-forge-builder.path;
  assert lib.elem evaluatorNix moduleConfig.systemd.services.crystal-forge-server.path;
  assert builtins.head moduleConfig.systemd.services.crystal-forge-builder.path == evaluatorNix;
  assert builtins.head moduleConfig.systemd.services.crystal-forge-server.path == evaluatorNix;
    pkgs.runCommand "crystal-forge-builder-evaluator-packaging" {
      nativeBuildInputs = [pkgs.coreutils pkgs.gnugrep pkgs.jq];
    } ''
      export HOME="$TMPDIR/home"
      export XDG_CACHE_HOME="$TMPDIR/cache"
      mkdir -p "$HOME" "$XDG_CACHE_HOME"

      component_wrapper=${componentBuilder}/bin/builder
      public_wrapper=${publicBuilder}/bin/builder
      evaluator_bin=${evaluatorNix}/bin
      scanner_bin=${pkgs.vulnix}/bin
      unrelated_nix_bin=${pkgs.nix}/bin

      test -x "$evaluator_bin/nix"
      test -x "$evaluator_bin/nix-store"
      test -x "$scanner_bin/vulnix"

      grep -Fq "$evaluator_bin" "$component_wrapper"
      grep -Fq "$scanner_bin" "$component_wrapper"
      grep -Fq "$evaluator_bin" "$public_wrapper"
      grep -Fq "$scanner_bin" "$public_wrapper"
      grep -Fq '${componentBuilder}/bin/builder' "$public_wrapper"

      if [ "$unrelated_nix_bin" != "$evaluator_bin" ]; then
        if grep -Fq "$unrelated_nix_bin" "$component_wrapper"; then
          echo "cf-builder-drv shadows evaluator Nix with $unrelated_nix_bin" >&2
          exit 1
        fi
        if grep -Fq "$unrelated_nix_bin" "$public_wrapper"; then
          echo "public builder shadows evaluator Nix with $unrelated_nix_bin" >&2
          exit 1
        fi
      fi

      packaged_version="$($evaluator_bin/nix --version)"
      packaged_scanner_version="$($scanner_bin/vulnix --version)"
      authoritative_version="$(${pkgs.nix-eval-jobs}/bin/nix-eval-jobs \
        --expr '{ probe = builtins.derivation { name = "crystal-forge-builder-evaluator-probe"; system = builtins.currentSystem; builder = "/bin/sh"; }; }' \
        --workers 1 \
        --meta \
        --apply '_: { nixVersion = builtins.nixVersion; }' \
        --option pure-eval false \
        --option allow-import-from-derivation true \
        | jq -er '.extraValue.nixVersion')"

      test "$packaged_version" = "nix (Nix) $authoritative_version"
      test -n "$packaged_scanner_version"

      # The sandbox has no daemon store. Use an explicit disposable chroot store
      # so this contract does not depend on the host's sandbox or daemon setup.
      probe_store="$TMPDIR/nix-root"
      input_addressed_drv="$($evaluator_bin/nix-instantiate --store "$probe_store" --expr \
        'builtins.derivation { name = "crystal-forge-derivation-json-probe"; system = "${system}"; builder = "/bin/sh"; args = []; }')"
      input_addressed_json="$($evaluator_bin/nix --store "$probe_store" --extra-experimental-features nix-command derivation show "$input_addressed_drv")"
      input_addressed_key="$(basename "$input_addressed_drv")"
      printf '%s' "$input_addressed_json" | jq -e \
        --arg key "$input_addressed_key" \
        '.version == 4
         and (.derivations | keys == [$key])
         and .derivations[$key].version == 4
         and (.derivations[$key].outputs.out.path | type == "string")
         and (.derivations[$key].outputs.out.path | startswith("/nix/store/") | not)' \
        >/dev/null

      second_input_addressed_drv="$($evaluator_bin/nix-instantiate --store "$probe_store" --expr \
        'builtins.derivation { name = "crystal-forge-derivation-json-probe-two"; system = "${system}"; builder = "/bin/sh"; args = []; }')"
      multi_derivation_json="$($evaluator_bin/nix --store "$probe_store" --extra-experimental-features nix-command derivation show \
        "$input_addressed_drv" "$second_input_addressed_drv")"
      second_input_addressed_key="$(basename "$second_input_addressed_drv")"
      printf '%s' "$multi_derivation_json" | jq -e \
        --arg first "$input_addressed_key" \
        --arg second "$second_input_addressed_key" \
        '.version == 4
         and (.derivations | length == 2)
         and (.derivations | has($first))
         and (.derivations | has($second))' \
        >/dev/null

      fixed_output_drv="$($evaluator_bin/nix-instantiate --store "$probe_store" --expr \
        'builtins.derivation { name = "crystal-forge-fixed-output-json-probe"; system = "${system}"; builder = "/bin/sh"; args = []; outputHashMode = "flat"; outputHashAlgo = "sha256"; outputHash = builtins.hashString "sha256" "probe"; }')"
      fixed_output_json="$($evaluator_bin/nix --store "$probe_store" --extra-experimental-features nix-command derivation show "$fixed_output_drv")"
      fixed_output_key="$(basename "$fixed_output_drv")"
      printf '%s' "$fixed_output_json" | jq -e \
        --arg key "$fixed_output_key" \
        '.version == 4
         and .derivations[$key].version == 4
         and (.derivations[$key].outputs.out | has("path") | not)' \
        >/dev/null
      fixed_output_path="$($evaluator_bin/nix-store --store "$probe_store" --query --binding out "$fixed_output_drv")"
      case "$fixed_output_path" in
        /nix/store/*) ;;
        *) exit 1 ;;
      esac

      printf '%s\n%s\n' "$packaged_version" "$packaged_scanner_version" > "$out"
    ''
