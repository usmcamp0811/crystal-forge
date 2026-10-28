# Immutable build prerequisites only. The scenario authors commits and nonces
# inside services; no deployed output, scan, or runtime identity is imported.
{ harness }: let
  inherit (harness) pkgs lib inputs;
  modulePath = inputs.self + "/modules/nixos/crystal-forge";
  deliveryFixtures = ../../integration-delivery/fixtures;
  agentRecipe = builtins.unsafeDiscardOutputDependency harness.packages.agent.drvPath;
  evaluator = pkgs.crystal-forge.default.evaluatorNix;
  evaluatorRecipe = builtins.unsafeDiscardOutputDependency evaluator.drvPath;
  evalJobsRecipe = builtins.unsafeDiscardOutputDependency pkgs.nix-eval-jobs.drvPath;
  baseline = inputs.nixpkgs.lib.nixosSystem {
    system = pkgs.stdenv.hostPlatform.system;
    inherit pkgs;
    specialArgs.sourceNonce = "baseline-preparation-only";
    modules = [
      (inputs.nixpkgs + "/nixos/lib/testing/nixos-test-base.nix")
      (inputs.nixpkgs + "/nixos/modules/virtualisation/qemu-vm.nix")
      modulePath (deliveryFixtures + "/configuration.nix")
    ];
  };
  source = pkgs.runCommand "assurance-source-fixture" {} ''
    mkdir -p "$out"
    cp ${./configuration.nix} "$out/configuration.nix"
    cp ${deliveryFixtures + "/target-baseline.nix"} "$out/target-baseline.nix"
    cp ${deliveryFixtures + "/target-network.nix"} "$out/target-network.nix"
    cp ${deliveryFixtures + "/network-isolation.nix"} "$out/network-isolation.nix"
    cp -r ${modulePath} "$out/crystal-forge"
    cp -r ${./vuln-data} "$out/vuln-data"
    printf '%s' baseline-preparation-only > "$out/nonce"
    printf '%s' 1.0 > "$out/package-version"
    printf '%s' false > "$out/agent-enabled"
    cat > "$out/flake.nix" <<'EOF'
    {
      inputs.nixpkgs.url = "github:nixos/nixpkgs/${inputs.nixpkgs.rev}";
      outputs = {nixpkgs, ...}: {
        nixosConfigurations.targetA = nixpkgs.lib.nixosSystem {
          system = "${pkgs.stdenv.hostPlatform.system}";
          pkgs = import nixpkgs {
            system = "${pkgs.stdenv.hostPlatform.system}";
            overlays = [(final: prev: {
              crystal-forge.default = {
                cf-agent-drv = { type = "derivation"; name = "${harness.packages.agent.name}";
                  drvPath = "${agentRecipe}";
                  outPath = builtins.appendContext "${harness.packages.agent}" { "${agentRecipe}" = { outputs = ["out"]; }; }; };
                evaluatorNix = { type = "derivation"; name = "${evaluator.name}";
                  drvPath = "${evaluatorRecipe}";
                  outPath = builtins.appendContext "${evaluator}" { "${evaluatorRecipe}" = { outputs = ["out"]; }; }; };
              };
              nix-eval-jobs = { type = "derivation"; name = "${pkgs.nix-eval-jobs.name}";
                drvPath = "${evalJobsRecipe}";
                outPath = builtins.appendContext "${pkgs.nix-eval-jobs}" { "${evalJobsRecipe}" = { outputs = ["out"]; }; };
                nix = final.crystal-forge.default.evaluatorNix;
              };
            })];
          };
          specialArgs.sourceNonce = builtins.readFile ./nonce;
          modules = [
            (nixpkgs + "/nixos/lib/testing/nixos-test-base.nix")
            (nixpkgs + "/nixos/modules/virtualisation/qemu-vm.nix")
            ./crystal-forge ./configuration.nix
            ({pkgs, lib, ...}: let
              demo = import ./vuln-data {inherit pkgs;};
              selectedDemo = if builtins.readFile ./package-version == "1.0"
                then demo.packages.vulnerable else demo.packages.corrected;
            in {
              services.crystal-forge.enable = lib.mkForce (builtins.readFile ./agent-enabled == "true");
              services.crystal-forge.client.enable = lib.mkForce (builtins.readFile ./agent-enabled == "true");
              # The shared VM instrumentation declares the unit's wantedBy.
              # Disable that empty unit too for the deliberately unadmitted input.
              systemd.services.crystal-forge-agent.enable = lib.mkForce (builtins.readFile ./agent-enabled == "true");
              environment.systemPackages = [selectedDemo];
              # The demo has no executable. Keep its real output in the runtime
              # closure even when buildEnv omits unselected share directories.
              environment.etc."cf-assurance-demo".source = "${"$"}{selectedDemo}/share/cf-assurance";
            })
          ];
        };
      };
    }
    EOF
    cat > "$out/flake.lock" <<'EOF'
    ${builtins.toJSON {
      version = 7; root = "root";
      nodes = {
        root.inputs.nixpkgs = "nixpkgs";
        nixpkgs = {
          locked = { type = "github"; owner = "nixos"; repo = "nixpkgs";
            inherit (inputs.nixpkgs) rev narHash lastModified; };
          original = { type = "github"; owner = "nixos"; repo = "nixpkgs"; rev = inputs.nixpkgs.rev; };
        };
      };
    }}
    EOF
  '';
in {
  inherit source baseline;
  prepared = {...}: {
    imports = [ (deliveryFixtures + "/network-isolation.nix") ];
    virtualisation.additionalPaths = [
      pkgs.path inputs.self baseline.config.system.build.toplevel
      baseline.config.system.build.toplevel.drvPath
    ];
    nix.settings = {
      experimental-features = ["nix-command" "flakes"];
      substituters = lib.mkForce []; builders = lib.mkForce []; fallback = false;
    };
  };
}
