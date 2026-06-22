{ lib, pkgs, ... }:
pkgs.rustPlatform.buildRustPackage {
  pname = "crystal-forge-oscal-fixture";
  version = "0.3.0";

  src = ./.;

  cargoLock = {
    lockFile = ./Cargo.lock;
  };

  meta = {
    description = "Deterministic OSCAL 1.1.2 Assessment Results fixture generator";
    mainProgram = "crystal-forge-oscal-fixture";
  };
}
