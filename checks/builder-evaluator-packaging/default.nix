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
  niks3 = packages.niks3;
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
  proxyAssertionMessage = "services.crystal-forge.server.trust_forwarded_builder_https requires nonempty services.crystal-forge.server.trustedProxyCidrs when services.crystal-forge.enable and services.crystal-forge.server.enable are true. Set narrow CIDRs for the reverse proxy's direct backend socket peer (Traefik peer IP /32 or /128), not builder/client IPs; no automatic or broad CIDR fallback is provided.";
  # Execute the real module wrapper with a recording binary, not a live server.
  # This avoids Rust builds, network listeners and database access in this proof.
  probeServer = pkgs.writeShellScriptBin "server" ''
    ${pkgs.jq}/bin/jq -n \
      --arg config "$CRYSTAL_FORGE_CONFIG" \
      --arg trustOverride "''${CRYSTAL_FORGE__SERVER__TRUST_FORWARDED_BUILDER_HTTPS-}" \
      '{config: $config, trustOverride: $trustOverride}'
  '';
  mkProxySystem = settings:
    inputs.nixpkgs.lib.nixosSystem {
      inherit system;
      modules = [
        inputs.self.nixosModules.crystal-forge
        {
          system.stateVersion = "26.05";
          boot.loader.grub.enable = false;
          fileSystems."/" = {
            device = "none";
            fsType = "tmpfs";
          };
          services.crystal-forge =
            lib.recursiveUpdate {
              enable = true;
              server = {
                enable = true;
                package = probeServer;
              };
              client.enable = false;
              build.enable = false;
              # Disable runtime key generation in the config-copy probe.
              auth.ssh_key_path = "/unused-module-probe-key";
            }
            settings;
        }
      ];
    };
  proxySystems = {
    falseEmpty = mkProxySystem {};
    trueEmpty = mkProxySystem {server.trust_forwarded_builder_https = true;};
    trueAllowed = mkProxySystem {
      server = {
        trust_forwarded_builder_https = true;
        trustedProxyCidrs = ["127.0.0.1/32" "::1/128"];
      };
    };
    globalDisabled = mkProxySystem {
      enable = false;
      server.trust_forwarded_builder_https = true;
    };
    serverDisabled = mkProxySystem {
      server = {
        enable = false;
        trust_forwarded_builder_https = true;
      };
      build.enable = true;
    };
  };
  failedAssertions = host:
    map (entry: entry.message)
    (lib.filter (entry: !entry.assertion) host.config.assertions);
  proxyCases =
    lib.mapAttrs (_: host: {
      failures = failedAssertions host;
      # Force the NixOS host assertion consumer, not only the assertions list.
      evaluates = (builtins.tryEval host.config.system.build.toplevel.drvPath).success;
    })
    proxySystems;
  validProxyConfigs = {
    falseEmpty = proxySystems.falseEmpty.config;
    trueAllowed = proxySystems.trueAllowed.config;
  };
  evaluatorSystems = {
    defaults = mkProxySystem {};
    explicitNull = mkProxySystem {server.eval_max_memory_mb = null;};
    explicit12288 = mkProxySystem {server.eval_max_memory_mb = 12288;};
    custom = mkProxySystem {
      server = {
        eval_workers = 3;
        eval_max_memory_mb = 2048;
        eval_memory_reserve_mb = 1024;
        eval_memory_max_percent = 70;
        eval_output_idle_timeout_secs = 123;
        eval_overall_timeout_secs = 456;
      };
    };
    workersZero = mkProxySystem {server.eval_workers = 0;};
    lowerBounds = mkProxySystem {
      server = {
        eval_max_memory_mb = 1;
        eval_memory_reserve_mb = 1;
        eval_memory_max_percent = 1;
        eval_output_idle_timeout_secs = 1;
        eval_overall_timeout_secs = 1;
      };
    };
    upperPercent = mkProxySystem {server.eval_memory_max_percent = 100;};
  };
  invalidEvaluatorValues = {
    eval_workers = [(-1) "2" null 1.5];
    eval_max_memory_mb = [0 (-1) "12288" 1.5];
    eval_memory_reserve_mb = [0 (-1) "4096" null];
    eval_memory_max_percent = [0 101 (-1) "85" null];
    eval_output_idle_timeout_secs = [0 (-1) "900" null];
    eval_overall_timeout_secs = [0 (-1) "3600" null];
  };
  # Force real module option values so rejected inputs exercise NixOS type
  # checking. No source-text inspection stands in for configuration behavior.
  evaluatorCases = {
    valid =
      lib.mapAttrs (_: host:
        (builtins.tryEval host.config.system.build.toplevel.drvPath).success)
      evaluatorSystems;
    invalid =
      lib.mapAttrs (option: values:
        map (value: let
          host = mkProxySystem {server.${option} = value;};
        in
          (builtins.tryEval (builtins.deepSeq
            host.config.services.crystal-forge.server.${option} true)).success)
        values)
      invalidEvaluatorValues;
  };
  generatedConfigCases = validProxyConfigs
    // lib.mapAttrs (_: host: host.config) evaluatorSystems;
  moduleValidation = assert proxyCases.falseEmpty.failures == [] && proxyCases.falseEmpty.evaluates;
  assert proxyCases.trueEmpty.failures == [proxyAssertionMessage] && !proxyCases.trueEmpty.evaluates;
  assert proxyCases.trueAllowed.failures == [] && proxyCases.trueAllowed.evaluates;
  assert proxyCases.globalDisabled.failures == [] && proxyCases.globalDisabled.evaluates;
  assert proxyCases.serverDisabled.failures == [] && proxyCases.serverDisabled.evaluates;
  assert lib.all (valid: valid) (builtins.attrValues evaluatorCases.valid);
  assert lib.all (results: lib.all (valid: !valid) results) (builtins.attrValues evaluatorCases.invalid);
    pkgs.runCommand "crystal-forge-proxy-module-validation" {
      nativeBuildInputs = [pkgs.bash pkgs.coreutils pkgs.jq pkgs.remarshal];
      passthru.evaluationResults = proxyCases // {evaluator = evaluatorCases;};
    } ''
      mkdir -p "$out"
      cp ${pkgs.writeText "proxy-assertion-results.json" (builtins.toJSON proxyCases)} "$out/assertions.json"
      cp ${pkgs.writeText "evaluator-option-results.json" (builtins.toJSON evaluatorCases)} "$out/evaluator-options.json"
      # Intercept only config-copy filesystem operations. Run the generated
      # module script unchanged; never write to the host's /var/lib paths.
      mkdir() { test "$*" = '-p /var/lib/crystal-forge'; }
      cp() {
        test "$#" = 2
        test "$2" = /var/lib/crystal-forge/config.toml
        command cp "$1" "$PROBE_CONFIG"
        printf '%s\n' "$2" > "$PROBE_DESTINATION"
      }
      chmod() {
        test "$*" = '600 /var/lib/crystal-forge/config.toml'
        command chmod 600 "$PROBE_CONFIG"
      }
      export -f mkdir cp chmod
      ${lib.concatStringsSep "\n" (lib.mapAttrsToList (name: hostConfig: let
          service = hostConfig.systemd.services.crystal-forge-server;
          preStart = pkgs.writeText "${name}-server-pre-start" service.preStart;
        in ''
          export PROBE_CONFIG="$out/${name}.toml"
          export PROBE_DESTINATION="$out/${name}-destination"
          generated=0
          while read -r command; do
            case "$command" in
              /nix/store/*-generate-crystal-forge-config-*)
                bash "$command"
                generated=$((generated + 1))
                ;;
            esac
          done < ${preStart}
          test "$generated" = 1
          toml2json "$PROBE_CONFIG" > "$out/${name}.json"
          test "$(command cat "$PROBE_DESTINATION")" = '${toString hostConfig.services.crystal-forge.configPath}'
          env CRYSTAL_FORGE_CONFIG=/ignored-inherited-config.toml \
            ${service.serviceConfig.ExecStart} > "$out/${name}-runtime.json"
          jq -e --arg path "$(command cat "$PROBE_DESTINATION")" \
            '.config == $path and .trustOverride == ""' "$out/${name}-runtime.json"
          env CRYSTAL_FORGE_CONFIG=/ignored-inherited-config.toml \
            CRYSTAL_FORGE__SERVER__TRUST_FORWARDED_BUILDER_HTTPS=false \
            ${service.serviceConfig.ExecStart} > "$out/${name}-override.json"
          jq -e '.config == "/var/lib/crystal-forge/config.toml" and .trustOverride == "false"' \
            "$out/${name}-override.json"
        '')
        generatedConfigCases)}
      jq -e '.server.trust_forwarded_builder_https == false and .server.trusted_proxy_cidrs == []
        and (.server | has("trustedProxyCidrs") | not)' "$out/falseEmpty.json"
      jq -e '.server.trust_forwarded_builder_https == true
        and .server.trusted_proxy_cidrs == ["127.0.0.1/32", "::1/128"]
        and (.server | has("trustedProxyCidrs") | not)' "$out/trueAllowed.json"
      for name in defaults explicitNull workersZero upperPercent; do
        jq -e '.server | has("eval_max_memory_mb") | not' "$out/$name.json"
      done
      for name in defaults explicitNull explicit12288 workersZero upperPercent; do
        jq -e '.server.eval_memory_reserve_mb == 4096
          and .server.eval_output_idle_timeout_secs == 900
          and .server.eval_overall_timeout_secs == 3600' "$out/$name.json"
      done
      for name in defaults explicitNull explicit12288; do
        jq -e '.server.eval_workers == 2 and .server.eval_memory_max_percent == 85' "$out/$name.json"
      done
      jq -e '.server.eval_max_memory_mb == 12288' "$out/explicit12288.json"
      jq -e '.server.eval_workers == 0 and .server.eval_memory_max_percent == 85' "$out/workersZero.json"
      jq -e '.server.eval_workers == 2 and .server.eval_memory_max_percent == 100' "$out/upperPercent.json"
      jq -e '.server.eval_workers == 3 and .server.eval_max_memory_mb == 2048
        and .server.eval_memory_reserve_mb == 1024 and .server.eval_memory_max_percent == 70
        and .server.eval_output_idle_timeout_secs == 123
        and .server.eval_overall_timeout_secs == 456' "$out/custom.json"
      jq -e '.server.eval_workers == 2 and .server.eval_max_memory_mb == 1
        and .server.eval_memory_reserve_mb == 1 and .server.eval_memory_max_percent == 1
        and .server.eval_output_idle_timeout_secs == 1
        and .server.eval_overall_timeout_secs == 1' "$out/lowerBounds.json"
    '';
in
  # INVARIANT: Both colocated services and the default builder package use the
  # Nix CLI linked to nix-eval-jobs. A package wrapper must not shadow this CLI.
  assert moduleConfig.services.crystal-forge.build.package == componentBuilder;
  assert evaluatorNix == packages.evaluatorNix;
  assert evaluatorNix.version == "2.34.8";
  assert lib.elem evaluatorNix moduleConfig.systemd.services.crystal-forge-builder.path;
  assert lib.elem evaluatorNix moduleConfig.systemd.services.crystal-forge-server.path;
  assert builtins.head moduleConfig.systemd.services.crystal-forge-builder.path == evaluatorNix;
  assert builtins.head moduleConfig.systemd.services.crystal-forge-server.path == evaluatorNix;
  assert lib.elem niks3 moduleConfig.systemd.services.crystal-forge-builder.path;
  assert lib.elem niks3 moduleConfig.systemd.services.crystal-forge-server.path;
    pkgs.runCommand "crystal-forge-builder-evaluator-packaging" {
      nativeBuildInputs = [pkgs.coreutils pkgs.gnugrep pkgs.gnused pkgs.jq pkgs.bash];
      passthru = {inherit moduleValidation;};
    } ''
      export HOME="$TMPDIR/home"
      export XDG_CACHE_HOME="$TMPDIR/cache"
      mkdir -p "$HOME" "$XDG_CACHE_HOME"
      test -f ${moduleValidation}/assertions.json

      component_wrapper=${componentBuilder}/bin/builder
      public_wrapper=${publicBuilder}/bin/builder
      evaluator_bin=${evaluatorNix}/bin
      scanner_bin=${pkgs.vulnix}/bin
      unrelated_nix_bin=${pkgs.nix}/bin

      test -x "$evaluator_bin/nix"
      test -x "$evaluator_bin/nix-store"
      test -x "$scanner_bin/vulnix"

      # Feature detection must require a registered setting: upstream Nix may
      # merely warn and continue when --option names an unknown setting.
      "$evaluator_bin/nix" --extra-experimental-features nix-command \
        config show --json | jq -e \
        'has("cf-netrc-authority") and .["cf-netrc-authority"].value == ""'
      test -z "$("$evaluator_bin/nix" --extra-experimental-features nix-command \
        config show cf-netrc-authority)"

      grep -Fq "$evaluator_bin" "$component_wrapper"
      grep -Fq "$scanner_bin" "$component_wrapper"
      grep -Fq "$evaluator_bin" "$public_wrapper"
      grep -Fq "$scanner_bin" "$public_wrapper"
      grep -Fq '${componentBuilder}/bin/builder' "$public_wrapper"
      grep -Fq '${niks3}/bin' "$component_wrapper"
      grep -Fq '${niks3}/bin' "$public_wrapper"

      # Execute the generated wrapper's PATH setup without starting a worker.
      # Also probe the core server wrapper: standalone packages must work
      # without the NixOS module's service PATH.
      for wrapper in "$component_wrapper" ${packages.cf-server-core-drv}/bin/server; do
        sed '/^exec /,$d' "$wrapper" > probe
        cat >> probe <<'PROBE'
      test "$(command -v nix)" = '${evaluatorNix}/bin/nix'
      test "$(command -v niks3)" = '${niks3}/bin/niks3'
      niks3 --help >/dev/null 2>&1
      PROBE
        bash -e probe
      done
      sed '\|${componentBuilder}/bin/builder|,$d' "$public_wrapper" > probe
      cat >> probe <<'PROBE'
      test "$(command -v nix)" = '${evaluatorNix}/bin/nix'
      test "$(command -v niks3)" = '${niks3}/bin/niks3'
      niks3 --help >/dev/null 2>&1
      PROBE
      bash -e probe
      grep -Fq '${evaluatorNix}/bin' ${niks3}/bin/niks3
      sed '/^exec /,$d' ${niks3}/bin/niks3 > probe
      printf '%s\n' 'test "$(command -v nix)" = "${evaluatorNix}/bin/nix"' >> probe
      bash -e probe
      env PATH=${lib.makeBinPath moduleConfig.systemd.services.crystal-forge-builder.path} \
        ${pkgs.bash}/bin/bash -ec 'test "$(command -v nix)" = "${evaluatorNix}/bin/nix"; test "$(command -v niks3)" = "${niks3}/bin/niks3"; command -v niks3; niks3 --help >/dev/null 2>&1'
      env PATH=${lib.makeBinPath moduleConfig.systemd.services.crystal-forge-server.path} \
        ${pkgs.bash}/bin/bash -ec 'test "$(command -v nix)" = "${evaluatorNix}/bin/nix"; test "$(command -v niks3)" = "${niks3}/bin/niks3"; command -v niks3; niks3 --help >/dev/null 2>&1'

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
