# D2's one in-place release transition. Immutable preparation is online; all
# runtime source ingestion, builds, publication and activation stay in this VLAN.
{harness, owner, name}: let
  inherit (harness) pkgs lib inputs;
  predecessor = inputs.upgrade-predecessor;
  system = pkgs.stdenv.hostPlatform.system;
  releasedModule = import ../fixtures/released-module.nix {inherit predecessor;};
  # Preserve the unmodified production module by content. Unrelated owner/test
  # commits must not change the immutable target recipe preparation identity.
  currentModule = builtins.path {
    path = inputs.self + "/modules/nixos/crystal-forge";
    name = "upgrade-current-module";
  };
  currentAgent = harness.packages.agent;
  targetArgs = {inherit predecessor currentModule currentAgent; sourceNonce = "preparation-only";};
  baseline = inputs.nixpkgs.lib.nixosSystem {
    inherit system pkgs;
    modules = [
      (inputs.nixpkgs + "/nixos/lib/testing/nixos-test-base.nix")
      (inputs.nixpkgs + "/nixos/modules/virtualisation/qemu-vm.nix")
      (import ../fixtures/target.nix targetArgs)
    ];
  };
  # Reuse Delivery's exact production evaluator overlay and recipe preparation.
  # Serialize derivations with output context, never host repository paths.
  package = drv: {
    # Keep JSON as recipe metadata. An outPath attribute coerces the whole set
    # to its output string and loses the recipe fields needed by pure eval.
    inherit (drv) name;
    recipe = builtins.unsafeDiscardOutputDependency drv.drvPath;
    output = "${drv}";
  };
  source = pkgs.runCommand "upgrade-version-transition-source" {} ''
    mkdir -p "$out/checks/integration-upgrade/fixtures" "$out/checks/integration-delivery/fixtures"
    cp ${pkgs.writeText "upgrade-target.nix" (builtins.readFile ../fixtures/target.nix)} "$out/checks/integration-upgrade/fixtures/target.nix"
    cp ${pkgs.writeText "upgrade-released-module.nix" (builtins.readFile ../fixtures/released-module.nix)} "$out/checks/integration-upgrade/fixtures/released-module.nix"
    cp ${pkgs.writeText "upgrade-target-network.nix" (builtins.readFile ../../integration-delivery/fixtures/target-network.nix)} "$out/checks/integration-delivery/fixtures/target-network.nix"
    cp ${pkgs.writeText "upgrade-network-isolation.nix" (builtins.readFile ../../integration-delivery/fixtures/network-isolation.nix)} "$out/checks/integration-delivery/fixtures/network-isolation.nix"
    cp -r ${currentModule} "$out/current-module"
    printf '%s' preparation-only > "$out/nonce"
    cat > "$out/packages.json" <<'EOF'
    ${builtins.toJSON {
      agent = package currentAgent;
      evaluator = package pkgs.crystal-forge.default.evaluatorNix;
      evalJobs = package pkgs.nix-eval-jobs;
    }}
    EOF
    cat > "$out/flake.nix" <<'EOF'
    {
      inputs.nixpkgs.url = "github:nixos/nixpkgs/${inputs.nixpkgs.rev}";
      inputs.upgrade-predecessor.url = "git+https://gitlab.com/crystal-forge/crystal-forge.git?rev=${predecessor.rev}";
      outputs = {nixpkgs, upgrade-predecessor, ...}: let
        packages = builtins.fromJSON (builtins.readFile ./packages.json);
        drv = p: {type = "derivation"; name = p.name; drvPath = p.recipe; outPath = builtins.appendContext p.output {
          "''${p.recipe}" = {outputs = ["out"];};
        };};
      in {
        nixosConfigurations.targetA = nixpkgs.lib.nixosSystem {
          system = "${system}";
          pkgs = import nixpkgs {system = "${system}"; overlays = [(final: prev: {
            crystal-forge.default = {
              cf-agent-drv = drv packages.agent;
              evaluatorNix = drv packages.evaluator;
            };
            nix-eval-jobs = (drv packages.evalJobs) // {nix = drv packages.evaluator;};
          })];};
          modules = let targetArgs = {
            predecessor = upgrade-predecessor;
            currentModule = ./current-module;
            currentAgent = drv packages.agent;
            sourceNonce = builtins.readFile ./nonce;
            useCurrentAgent = builtins.pathExists ./current-agent;
          }; in [
            (nixpkgs + "/nixos/lib/testing/nixos-test-base.nix")
            (nixpkgs + "/nixos/modules/virtualisation/qemu-vm.nix")
            (import ./checks/integration-upgrade/fixtures/target.nix targetArgs)
          ];
        };
      };
    }
    EOF
    # Preserve the entire exact input graph, replacing only this fixture root.
    cat > "$out/flake.lock" <<'EOF'
    ${builtins.toJSON (let
      lock = builtins.fromJSON (builtins.readFile ../../../flake.lock);
    in lock // {nodes = lock.nodes // {root.inputs = {
      nixpkgs = lock.nodes.root.inputs.nixpkgs;
      upgrade-predecessor = lock.nodes.root.inputs.upgrade-predecessor;
    }; ${lock.nodes.root.inputs.nixpkgs} = lock.nodes.${lock.nodes.root.inputs.nixpkgs} // {
      # Match the exact-revision URL declared in this generated source. A
      # branch-shaped original would require an offline lock-file update.
      original = {type = "github"; owner = "nixos"; repo = "nixpkgs"; rev = inputs.nixpkgs.rev;};
    };};})}
    EOF
  '';
  inputSources = flake: [flake] ++ lib.concatMap inputSources (builtins.attrValues (flake.inputs or {}));
  prepared = {...}: {
    imports = [../../integration-delivery/fixtures/network-isolation.nix];
    virtualisation.additionalPaths = [
      # The source vendors currentModule and has only the two declared inputs;
      # current package recipes already carry their exact source dependencies.
      pkgs.path
      baseline.config.system.build.toplevel baseline.config.system.build.toplevel.drvPath
    ] ++ inputSources predecessor;
    nix.settings = {
      experimental-features = ["nix-command" "flakes"];
      substituters = lib.mkForce [];
      builders = lib.mkForce [];
      fallback = false;
    };
  };
  builder = {...}: {
    imports = [releasedModule prepared];
    virtualisation.memorySize = 2048;
    virtualisation.cores = 2;
    virtualisation.diskSize = 12288;
    services.crystal-forge = {
      enable = true;
      local-database = false;
      server.enable = false;
      # The real release's API builder still needs its own DB pool. This
      # phase-only dependency is removed before the current server can start.
      database.host = "controller";
      build = {
        enable = true;
        api_mode = true;
        server_url = "http://controller:3000";
        use_substitutes = false;
        # The release incorrectly passes --offline to legacy nix-store. Keep
        # that flag off without changing the genuine binary. This VM remains
        # offline through egress rejection, no substitutes/builders and no
        # fallback. Current builders use their supported offline option below.
        offline = false;
        max_concurrent_derivations = 1;
        max_jobs = 1;
        cores_per_job = 1;
      };
      # The release signs and queues publication from its normal static cache
      # configuration, even in API mode. This is only the predecessor phase.
      cache = {
        push_to = "ssh-ng://cache@services";
        push_after_build = true;
        signing_key = "/var/lib/upgrade-credentials/cache-sign.key";
      };
    };
    environment.systemPackages = [pkgs.openssh pkgs.nodejs predecessor.packages.${system}.cf-keygen];
    environment.etc."cf-it/signed-request.cjs".source = ../fixtures/signed-request.cjs;
    systemd.services.crystal-forge-builder.wantedBy = lib.mkForce [];
    systemd.services.crystal-forge-builder.path = [pkgs.openssh];
    # The release module supplies key/server settings, but its real API client
    # also requires the registered UUID and explicit API-mode environment.
    systemd.services.crystal-forge-builder.serviceConfig.EnvironmentFile = [
      "-/var/lib/crystal-forge/release-builder.env"
    ];
    specialisation.current.configuration = {
      disabledModules = [{key = "upgrade-released-module";}];
      imports = [currentModule];
      services.crystal-forge.build = {
        package = harness.packages.builder;
        offline = lib.mkForce true;
        supported_execution_strategies = ["server_derivation"];
        cve_scanning_enabled = false;
      };
      # Current publication authority comes from the server-issued job context.
      services.crystal-forge.cache = {
        push_to = lib.mkForce null;
        push_after_build = lib.mkForce false;
        signing_key = lib.mkForce null;
      };
      systemd.services.crystal-forge-builder.serviceConfig.EnvironmentFile = lib.mkForce [
        "-/var/lib/crystal-forge/.config/crystal-forge-attic.env"
      ];
    };
  };
in assert predecessor.rev == "f9dd6b9c79530d3f911203645207bf6251552aba";
harness.mkScenario {
  inherit owner name;
  contracts = ["upgrade.continuity"];
  nodes = {
    controller = {...}: {
      imports = [releasedModule prepared harness.browser.module];
      virtualisation.memorySize = 4096;
      virtualisation.cores = 2;
      virtualisation.diskSize = 16384;
      networking.firewall.allowedTCPPorts = [3000 5432];
      services.crystal-forge = {
        enable = true;
        local-database = true;
        # The released SQLx URL serializer needs an explicit TCP host. The
        # normal module already allows this loopback connection to the same DB.
        database.host = "127.0.0.1";
        server = {enable = true; host = "0.0.0.0"; eval_workers = 1; eval_max_memory_mb = 1024;};
        build.enable = false;
        flakes = {flake_polling_interval = "1h"; commit_evaluation_interval = "2s"; build_processing_interval = "2s";};
      };
      services.postgresql = {
        enableTCPIP = true;
        # Disposable VLAN only. Cutover removes these rules before migration.
        authentication = lib.mkAfter ''
          host crystal_forge crystal_forge 192.168.1.1/32 trust
          host crystal_forge crystal_forge 192.168.1.2/32 trust
          host crystal_forge crystal_forge 2001:db8:1::1/128 trust
          host crystal_forge crystal_forge 2001:db8:1::2/128 trust
        '';
      };
      # Current NixOS separates database/user provisioning from the PostgreSQL
      # daemon unit. Release services must wait for that normal setup target.
      systemd.services.crystal-forge-server.after = ["postgresql.target"];
      # The release selects local routes through AUTH_MODE. Its module's
      # auth_mode option alone does not emit this environment value for local.
      systemd.services.crystal-forge-server.environment.AUTH_MODE = "local";
      systemd.services.crystal-forge-postgres-jobs.after = ["postgresql.target"];
      environment.systemPackages = [pkgs.git pkgs.postgresql pkgs.openssh];
      specialisation.current.configuration = {
        disabledModules = [{key = "upgrade-released-module";}];
        imports = [currentModule];
        services.crystal-forge.server.package = harness.packages.server;
        services.crystal-forge.build.remote_execution_strategy = "server_derivation";
        services.postgresql.authentication = lib.mkForce ''
          local all postgres peer
          local crystal_forge crystal_forge peer map=crystal-forge-map
          host crystal_forge crystal_forge 127.0.0.1/32 trust
          host crystal_forge crystal_forge ::1/128 trust
          host all all 0.0.0.0/0 reject
          host all all ::/0 reject
        '';
      };
    };
    services = {...}: {
      imports = [(harness.roles.services {}) prepared];
      environment.etc."cf-it/source".source = source;
    };
    builderA = builder;
    builderB = builder;
    targetA = {...}: {
      imports = [(import ../fixtures/target.nix targetArgs)];
      virtualisation.additionalPaths = [baseline.config.system.build.toplevel];
    };
  };
  facts = {
    admin = harness.facts.localUserFromDesign {id = "u1"; username = "mreyes"; password = "cf-it-upgrade-admin-1";};
    predecessor = {
      tag = "v0.3.1";
      revision = predecessor.rev;
      source = "${predecessor}";
      manifestVersion = predecessor.packages.${system}.default.version;
      agent = "${predecessor.packages.${system}.agent}/bin/agent";
      server = "${predecessor.packages.${system}.server}/bin/server";
      builder = "${predecessor.packages.${system}.server}/bin/builder";
    };
    current = {agent = "${currentAgent}/bin/agent"; server = "${harness.packages.server}/bin/server"; builder = "${harness.packages.builder}/bin/builder";};
    flake = {name = "upgrade-source"; url = "http://services/upgrade.git"; branch = "main";};
    target = "targetA";
  };
  script = ./version-transition.py;
  browserSpecs.version-transition = ./version-transition.cjs;
  timeout = 3600;
}
