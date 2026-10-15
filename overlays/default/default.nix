{
  channels,
  process-compose-flake,
  nixos-compose,
  ...
}: final: prev: let
  # nixpkgs reshuffled filetransfer.cc in Nix 2.35 (e.g. request.uri became
  # request.displayUri()), so the 2.34 patch no longer applies there. Pick the
  # patch matching the Nix that the consuming nixpkgs ships.
  netrcAuthorityPatch =
    if prev.lib.versionAtLeast prev.nix-eval-jobs.nix.version "2.35"
    then ../../packages/default/patches/nix-cf-netrc-authority-2.35.patch
    else ../../packages/default/patches/nix-cf-netrc-authority.patch;

  # SECURITY: CF Basic readers need an opt-in authority guard in native Nix.
  # Rebuild the evaluator and CLI from one component scope: adding a setting
  # changes FileTransferSettings layout, so mixing old libraries is unsafe.
  evaluatorComponents =
    (prev.nix-eval-jobs.nixComponents.appendPatches [
      netrcAuthorityPatch
    ]).overrideAllMesonComponents (_: _: {
      # Keep the evaluator's exact upstream version contract. Nixpkgs normally
      # adds a patch-count suffix; the source stays at the nixpkgs-provided
      # Nix version (2.34.8 on the repo's own pin, 2.35.x on newer nixpkgs).
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
