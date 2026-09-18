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
      unrelated_nix_bin=${pkgs.nix}/bin

      grep -Fq "$evaluator_bin" "$component_wrapper"
      grep -Fq "$evaluator_bin" "$public_wrapper"
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
      authoritative_version="$(${pkgs.nix-eval-jobs}/bin/nix-eval-jobs \
        --expr '{ probe = builtins.derivation { name = "crystal-forge-builder-evaluator-probe"; system = builtins.currentSystem; builder = "/bin/sh"; }; }' \
        --workers 1 \
        --meta \
        --apply '_: { nixVersion = builtins.nixVersion; }' \
        --option pure-eval false \
        --option allow-import-from-derivation true \
        | jq -er '.extraValue.nixVersion')"

      test "$packaged_version" = "nix (Nix) $authoritative_version"
      printf '%s\n' "$packaged_version" > "$out"
    ''
