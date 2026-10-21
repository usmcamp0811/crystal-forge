# One evolving controller/services/two-builder/target topology. Source ingestion,
# native evaluation and authorized execution share exact identities through
# receipts. Publication is a real prerequisite for native builder completion;
# this scenario also proves manual activation and retained History rollback.
# Other adapters, activation modes and restart/drift recovery remain open.
{harness, owner, name}: let
  inherit (harness) pkgs lib inputs;
  modulePath = inputs.self + "/modules/nixos/crystal-forge";
  configuration = ../fixtures/configuration.nix;
  # Recipes are data in the source generator. Avoid expanding their deep
  # contexts here; complete cold-store preparation belongs to the baseline
  # below, rather than to the generated Git source.
  agentRecipe = builtins.unsafeDiscardOutputDependency harness.packages.agent.drvPath;
  evaluatorRecipe = builtins.unsafeDiscardOutputDependency pkgs.crystal-forge.default.evaluatorNix.drvPath;
  evalJobsRecipe = builtins.unsafeDiscardOutputDependency pkgs.nix-eval-jobs.drvPath;
  baseline = inputs.nixpkgs.lib.nixosSystem {
    system = pkgs.stdenv.hostPlatform.system;
    inherit pkgs;
    specialArgs.sourceNonce = "baseline-preparation-only";
    modules = [
      (inputs.nixpkgs + "/nixos/lib/testing/nixos-test-base.nix")
      (inputs.nixpkgs + "/nixos/modules/virtualisation/qemu-vm.nix")
      modulePath configuration
    ];
  };
  # Pin the same immutable input identity as the product flake and preload its
  # exact store source on the evaluator/builders. No registry rewrite is used.
  source = pkgs.runCommand "delivery-source-fixture" {} ''
    mkdir -p "$out"
    cp ${configuration} "$out/configuration.nix"
    cp ${../fixtures/target-baseline.nix} "$out/target-baseline.nix"
    cp ${../fixtures/target-network.nix} "$out/target-network.nix"
    cp ${../fixtures/network-isolation.nix} "$out/network-isolation.nix"
    # Vendor the unchanged production module into the authored source. Pure
    # evaluation must not import an undeclared absolute host/worktree path.
    cp -r ${modulePath} "$out/crystal-forge"
    printf '%s' baseline-preparation-only > "$out/nonce"
    cat > "$out/flake.nix" <<'EOF'
    {
      inputs.nixpkgs.url = "github:nixos/nixpkgs/${inputs.nixpkgs.rev}";
      outputs = {nixpkgs, ...}: {
        nixosConfigurations.targetA = nixpkgs.lib.nixosSystem {
          system = "${pkgs.stdenv.hostPlatform.system}";
          pkgs = import nixpkgs {
            system = "${pkgs.stdenv.hostPlatform.system}";
            overlays = [(final: prev: { crystal-forge.default = {
              # Serialize references to the actual prepared production outputs.
              # Package options require derivation metadata, not bare paths
              # (which coerce through storePath, forbidden in pure evaluation).
              cf-agent-drv = {
                type = "derivation";
                name = "${harness.packages.agent.name}";
                drvPath = "${agentRecipe}";
                outPath = builtins.appendContext "${harness.packages.agent}" {
                  "${agentRecipe}" = {outputs = ["out"];};
                };
              };
              evaluatorNix = {
                type = "derivation";
                name = "${pkgs.crystal-forge.default.evaluatorNix.name}";
                drvPath = "${evaluatorRecipe}";
                outPath = builtins.appendContext "${pkgs.crystal-forge.default.evaluatorNix}" {
                  "${evaluatorRecipe}" = {outputs = ["out"];};
                };
              };
            };
              # Match the production overlay, not bare nixpkgs' evaluator.
              # This also makes nix-fast-build identical to the prepared one;
              # otherwise realization rebuilds a second Nix library/toolchain.
              nix-eval-jobs = {
                type = "derivation";
                name = "${pkgs.nix-eval-jobs.name}";
                drvPath = "${evalJobsRecipe}";
                outPath = builtins.appendContext "${pkgs.nix-eval-jobs}" {
                  "${evalJobsRecipe}" = {outputs = ["out"];};
                };
                nix = final.crystal-forge.default.evaluatorNix;
              };
            })];
          };
          specialArgs.sourceNonce = builtins.readFile ./nonce;
          modules = [
            (nixpkgs + "/nixos/lib/testing/nixos-test-base.nix")
            (nixpkgs + "/nixos/modules/virtualisation/qemu-vm.nix")
            ./crystal-forge ./configuration.nix
          ];
        };
      };
    }
    EOF
    cat > "$out/flake.lock" <<'EOF'
    ${builtins.toJSON {
      version = 7;
      root = "root";
      nodes = {
        root.inputs.nixpkgs = "nixpkgs";
        nixpkgs = {
          locked = {
            type = "github";
            owner = "nixos";
            repo = "nixpkgs";
            rev = inputs.nixpkgs.rev;
            narHash = inputs.nixpkgs.narHash;
            lastModified = inputs.nixpkgs.lastModified;
          };
          original = {type = "github"; owner = "nixos"; repo = "nixpkgs"; rev = inputs.nixpkgs.rev;};
        };
      };
    }}
    EOF
  '';
  offlineNetwork = ../fixtures/network-isolation.nix;
  prepared = {...}: {
    imports = [offlineNetwork];
    # Raw derivation-archive transport needs a valid full recipe closure. The
    # deep baseline context materializes bootstrap source objects too; an
    # opaque recipe worked only on a warm host and failed cold-store export.
    # All roles share this one immutable preparation, never the nonce output.
    virtualisation.additionalPaths = [
      pkgs.path inputs.self baseline.config.system.build.toplevel
      baseline.config.system.build.toplevel.drvPath
    ];
    nix.settings = {
      experimental-features = ["nix-command" "flakes"];
      substituters = lib.mkForce [];
      builders = lib.mkForce [];
      fallback = false;
    };
  };
in harness.mkScenario {
  inherit owner name;
  contracts = ["delivery.evaluation" "delivery.build" "delivery.publication" "delivery.activation.modes" "delivery.activation.recovery"];
  nodes = {
    controller = { ... }: {
      imports = [(harness.roles.controller {browser = true;}) prepared];
      environment.systemPackages = [pkgs.git];
      # The production automatic reserve is 4 GiB, equal to this entire VM.
      # Use its documented fixed-worker budget for one configuration instead.
      services.crystal-forge.server = {eval_workers = 1; eval_max_memory_mb = 1024;};
      services.crystal-forge.build.remote_execution_strategy = "server_derivation";
      services.crystal-forge.flakes = {
        flake_polling_interval = "1h";
        commit_evaluation_interval = "2s";
        build_processing_interval = "2s";
      };
    };
    services = {...}: {
      # Share only immutable preparation, including input-closure metadata.
      # Otherwise first publication needlessly copies the entire toolchain
      # into this writable cache. The runtime nonce output is still absent.
      imports = [(harness.roles.services {}) prepared ../fixtures/signature-receiver.nix];
      environment.etc."cf-it/source".source = source;
      networking.firewall.allowedTCPPorts = [9080];
      systemd.services.network-probe = {
        description = "Controlled outside-boundary connectivity probe";
        wantedBy = ["multi-user.target"];
        after = ["network.target"];
        serviceConfig = {
          DynamicUser = true;
          ExecStart = "${pkgs.python3}/bin/python3 ${../fixtures/network-probe.py}";
          Restart = "on-failure";
        };
      };
    };
    builderA = {...}: {
      imports = [(harness.roles.builder {name = "builderA";}) prepared];
      # Retain native copy diagnostics for this controlled local SSH store.
      # Private keys remain files; no external cache credentials are supplied.
      systemd.services.crystal-forge-builder.environment.RUST_LOG = lib.mkForce "info,cf_builder::derivations::cache=debug";
    };
    builderB = {...}: {imports = [(harness.roles.builder {name = "builderB";}) prepared];};
    targetA = {...}: {
      imports = [(harness.roles.target {}) offlineNetwork ../fixtures/target-baseline.nix ../fixtures/cold-acquisition.nix];
      # Share immutable runtime prerequisites, not recipes or runtime candidates.
      # Warm SSH stores do not merge signatures onto already valid baseline
      # paths. Ordinary deployment proves cold candidate acquisition. A separate
      # empty rooted store in this VM proves whole-closure publisher trust.
      virtualisation.additionalPaths = [baseline.config.system.build.toplevel];
    };
  };
  facts = {
    admin = harness.facts.localUserFromDesign {
      id = "u1"; username = "mreyes"; password = "cf-it-delivery-admin-1";
    };
    flake = {name = "delivery-source"; url = "http://services/delivery.git"; branch = "main";};
    target = "targetA";
    cacheUrl = "ssh-ng://cache@services";
    receiverNix = "${pkgs.crystal-forge.default.evaluatorNix}";
  };
  script = pkgs.writeText "delivery-lifecycle.py" (
     builtins.readFile ./delivery-lifecycle.py + "\n"
     + builtins.readFile ./fragments/source-build.py + "\n"
      + builtins.readFile ./fragments/publication.py + "\n"
      + builtins.readFile ./fragments/cache-acquisition.py + "\n"
     + builtins.readFile ./fragments/network-boundary.py + "\n"
     + builtins.readFile ./fragments/activation.py + "\n"
     + builtins.readFile ./fragments/recovery.py
  );
  browserSpecs = let
    activation = ./fragments/activation.cjs;
    wrapper = method: phase: pkgs.writeText "delivery-${method}-${phase}.cjs" ''
      exports.run = ctx => require("${activation}").${method}(ctx, "${phase}");
    '';
  in {
    deploy-first = wrapper "deploy" "first";
    deploy-second = wrapper "deploy" "second";
    inspect-first = wrapper "inspect" "first";
    inspect-second = wrapper "inspect" "second";
    inspect-rollback = wrapper "inspect" "rollback";
    started-first = wrapper "started" "first";
    started-second = wrapper "started" "second";
    started-rollback = wrapper "started" "rollback";
    rollback = pkgs.writeText "delivery-rollback.cjs" ''
      exports.run = ctx => require("${./fragments/recovery.cjs}").run(ctx, require("${activation}"));
    '';
  };
  timeout = 1800;
}
