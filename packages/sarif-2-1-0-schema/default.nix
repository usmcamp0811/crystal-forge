{ lib, pkgs, ... }:
pkgs.stdenvNoCC.mkDerivation {
  pname = "sarif-2.1.0-schema";
  version = "2.1.0-errata01";
  src = ../../schemas/sarif-2.1.0;
  dontBuild = true;
  installPhase = ''
    mkdir -p "$out"
    cp "$src"/*.json "$out/"
  '';
  meta = {
    description = "Vendored OASIS SARIF 2.1.0 Errata 01 JSON schema";
  };
}
