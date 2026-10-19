# One evolving controller/services/two-builder/target topology. Source ingestion,
# native evaluation and authorized execution share exact identities through
# receipts. Publication is a real prerequisite for native builder completion;
# this initial proof does not establish adapter, deployment or recovery coverage.
{harness, owner, name}: let
  inherit (harness) pkgs lib inputs;
  modulePath = inputs.self + "/modules/nixos/crystal-forge";
  configuration = ../fixtures/configuration.nix;
  baseline = inputs.nixpkgs.lib.nixosSystem {
    system = pkgs.stdenv.hostPlatform.system;
    inherit pkgs;
    specialArgs.sourceNonce = "baseline-preparation-only";
    modules = [modulePath configuration];
  };
  # Pin the same immutable input identity as the product flake and preload its
  # exact store source on the evaluator/builders. No registry rewrite is used.
  source = pkgs.runCommand "delivery-source-fixture" {} ''
    mkdir -p "$out"
    cp ${configuration} "$out/configuration.nix"
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
                drvPath = "${harness.packages.agent.drvPath}";
                outPath = builtins.appendContext "${harness.packages.agent}" {
                  "${harness.packages.agent.drvPath}" = {outputs = ["out"];};
                };
              };
              evaluatorNix = {
                type = "derivation";
                name = "${pkgs.crystal-forge.default.evaluatorNix.name}";
                drvPath = "${pkgs.crystal-forge.default.evaluatorNix.drvPath}";
                outPath = builtins.appendContext "${pkgs.crystal-forge.default.evaluatorNix}" {
                  "${pkgs.crystal-forge.default.evaluatorNix.drvPath}" = {outputs = ["out"];};
                };
              };
            }; })];
          };
          specialArgs.sourceNonce = builtins.readFile ./nonce;
          modules = [./crystal-forge ./configuration.nix];
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
  offlineNetwork = {...}: {
    environment.systemPackages = [pkgs.iptables];
    # eth0 is QEMU's public/NAT interface; role traffic uses the test VLAN.
    # Permit boot DHCP/IPv6 link control, but no external fixture acquisition.
    networking.firewall.extraCommands = ''
      iptables -A OUTPUT -o eth0 -p udp --dport 67 -d 255.255.255.255/32 -j ACCEPT
      iptables -A OUTPUT -o eth0 -p udp --dport 67 -d 10.0.2.2/32 -j ACCEPT
      iptables -A OUTPUT -o eth0 -j REJECT
      ip6tables -A OUTPUT -o eth0 -p ipv6-icmp -d fe80::/10 -j ACCEPT
      ip6tables -A OUTPUT -o eth0 -p ipv6-icmp -d ff02::/16 -j ACCEPT
      ip6tables -A OUTPUT -o eth0 -j REJECT
    '';
  };
  prepared = {...}: {
    imports = [offlineNetwork];
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
  contracts = ["delivery.evaluation" "delivery.build" "delivery.publication"];
  nodes = {
    controller = { ... }: {
      imports = [(harness.roles.controller {}) prepared];
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
      imports = [(harness.roles.services {}) prepared];
      environment.etc."cf-it/source".source = source;
    };
    builderA = {...}: {
      imports = [(harness.roles.builder {name = "builderA";}) prepared];
      # Retain native copy diagnostics for this controlled local SSH store.
      # Private keys remain files; no external cache credentials are supplied.
      systemd.services.crystal-forge-builder.environment.RUST_LOG = lib.mkForce "info,cf_builder::derivations::cache=debug";
    };
    builderB = {...}: {imports = [(harness.roles.builder {name = "builderB";}) prepared];};
    targetA = {...}: {imports = [(harness.roles.target {}) offlineNetwork];};
  };
  facts = {
    admin = harness.facts.localUserFromDesign {
      id = "u1"; username = "mreyes"; password = "cf-it-delivery-admin-1";
    };
    flake = {name = "delivery-source"; url = "http://services/delivery.git"; branch = "main";};
    target = "targetA";
    cacheUrl = "ssh-ng://cache@services";
  };
  script = pkgs.writeText "delivery-lifecycle.py" (
    builtins.readFile ./delivery-lifecycle.py + "\n"
    + builtins.readFile ./fragments/source-build.py
  );
  timeout = 1800;
}
