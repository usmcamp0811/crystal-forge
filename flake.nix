{
  description = "Simple flake exporting a Rust package";

  # Lets `nix develop .#devenv` fetch the devenv CLI itself (and its own
  # dependency closure) from devenv's binary cache instead of building it
  # from source. Purely a speed optimization for that one additive shell;
  # every other input/output in this flake is unaffected.
  nixConfig = {
    extra-substituters = [ "https://devenv.cachix.org" ];
    extra-trusted-public-keys = [
      "devenv.cachix.org-1:w1cLUi8dv3hnoSPGAuibQv+f9TZLr6cv/Hm9XgU50cw="
    ];
  };

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/release-26.05";
    flake-utils.url = "github:numtide/flake-utils";
    process-compose-flake.url = "github:Platonic-Systems/process-compose-flake";
    services-flake.url = "github:juspay/services-flake";
    crane.url = "github:ipetkov/crane";
    snowfall-lib = {
      url = "github:snowfallorg/lib";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nixos-compose = {
      url = "github:oar-team/nixos-compose/25.05";
      inputs.nixpkgs.url = "github:nixos/nixpkgs/release-25.05";
    };
    # Stable nixpkgs for pre-built browser binaries (screenshots in web-ui check)
    nixpkgs-stable.url = "github:NixOS/nixpkgs/nixos-25.05";
    # Dioxus CLI pinned to the application-compatible 0.7.3 release for the
    # incremental local UI development harness.
    nixpkgs-dioxus-cli.url = "github:NixOS/nixpkgs/09061f748ee21f68a089cd5d91ec1859cd93d0be";
    # devenv (TASK-462.1): provides the real `devenv` CLI for the additive
    # `devShells.devenv` shell below. The process definitions it drives
    # live in this repository's own `devenv.yaml`/`devenv.nix` (devenv's
    # native project format), not here; see devenv.nix's top comment for
    # why flake integration (`devenv.lib.mkShell`) is not used instead.
    devenv.url = "github:cachix/devenv";
  };

  outputs = inputs:
    let
      lib = inputs.snowfall-lib.mkLib {
        inherit inputs;
        src = ./.;
        snowfall = {
          meta = {
            name = "crystal-forge";
            title = "Crystal Forge";
          };
          namespace = "crystal-forge";
        };
      };
    in lib.mkFlake {
      channels-config = { allowUnfree = true; };
      outputs-builder = channels: {
        packages = {
          agent = channels.nixpkgs.crystal-forge.default.agent;
          server = channels.nixpkgs.crystal-forge.default.server;
          builder = channels.nixpkgs.crystal-forge.default.builder;
          cf-keygen = channels.nixpkgs.crystal-forge.default.cf-keygen;
          test-agent = channels.nixpkgs.crystal-forge.default.test-agent;
          web-ui = channels.nixpkgs.crystal-forge.web-ui;
          oscal-fixture = channels.nixpkgs.crystal-forge.oscal-fixture;
          oscal-1-1-2-schemas = channels.nixpkgs.crystal-forge.oscal-1-1-2-schemas;
          xccdf-1-2-schemas = channels.nixpkgs.crystal-forge.xccdf-1-2-schemas;
          nixos-options-metadata = channels.nixpkgs.crystal-forge.nixos-options-metadata;
        };
        apps.generate-design-targets = import ./apps/generate-design-targets/default.nix {
          lib = channels.nixpkgs.lib;
          pkgs = channels.nixpkgs;
          inherit inputs;
        };

        # nix build .#design-targets  → ./result/<view>--<theme>.design.png
        packages.design-targets = import ./packages/design-targets/default.nix {
          lib = channels.nixpkgs.lib;
          pkgs = channels.nixpkgs;
          inherit inputs;
        };

        # nix build .#ui-screenshots  → ./result/<view>--<theme>.png (Dioxus UI, fixture-driven)
        packages.ui-screenshots = import ./checks/ui-screenshots/default.nix {
          lib = channels.nixpkgs.lib;
          pkgs = channels.nixpkgs;
          inherit inputs;
        };

        # nix develop .#devenv (TASK-462.1): additive, alongside the
        # existing `devShells.default` (shells/default/default.nix), which
        # this output does not modify, replace, or remove. This shell only
        # puts the real `devenv` CLI on `PATH`; it does not itself define
        # or evaluate any devenv processes. Run `devenv up` from this
        # worktree's root once inside it to start the isolated
        # PostgreSQL/API/web UI stack defined in this repository's
        # `devenv.yaml`/`devenv.nix`. See docs/agents/devenv-workflow.md.
        devShells.devenv = channels.nixpkgs.mkShell {
          packages = [ inputs.devenv.packages.${channels.nixpkgs.stdenv.hostPlatform.system}.default ];
          shellHook = ''
            echo "🧪 Crystal Forge devenv workflow (TASK-462.1, additive, parallel-worktree-safe)"
            echo ""
            echo "  devenv up      → start PostgreSQL + API server + web UI dev server"
            echo "                   with per-worktree dynamic ports (devenv.nix)"
            echo "  devenv up -d   → same, detached"
            echo "  devenv processes list  → show this worktree's resolved ports"
            echo "  devenv down    → stop only this worktree's stack"
            echo ""
            echo "  Does not replace: nix develop / run-ui-dev / process-compose."
            echo "  Docs: docs/agents/devenv-workflow.md"
          '';
        };
      };
    };
}
