{
  channels,
  process-compose-flake,
  nixos-compose,
  ...
}: final: prev: let
  # SECURITY: CF Basic readers need an opt-in authority guard in native Nix.
  # Rebuild the evaluator and CLI from one component scope: adding a setting
  # changes FileTransferSettings layout, so mixing old libraries is unsafe.
  evaluatorComponents =
    (prev.nix-eval-jobs.nixComponents.appendPatches [
      ../../packages/default/patches/nix-cf-netrc-authority.patch
    ]).overrideAllMesonComponents (_: _: {
      # Keep the evaluator's exact upstream version contract. Nixpkgs normally
      # adds a patch-count suffix; the source remains pinned to Nix 2.34.8.
      version = prev.nix-eval-jobs.nix.version;
    });
in {
  nix-eval-jobs = prev.nix-eval-jobs.override {
    nixComponents = evaluatorComponents;
  };
  process-compose-flake = import process-compose-flake.lib {pkgs = final;};
  nxc-lib = nixos-compose.lib;
  nxc = nixos-compose.packages.${prev.system}.nixos-compose;

}
