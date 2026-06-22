{ lib, pkgs, ... }:
pkgs.stdenvNoCC.mkDerivation {
  pname = "oscal-1.1.2-schemas";
  version = "1.1.2";
  dontBuild = true;
  installPhase = ''
    mkdir -p "$out"
    cp ${../../schemas/oscal-1.1.2}/*.json "$out/"
  '';
  meta = {
    description = "Vendored NIST OSCAL 1.1.2 JSON schemas";
  };
}
