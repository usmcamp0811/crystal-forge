{
  lib,
  pkgs,
  ...
}: let
  pname = "server";
  crystal-forge-server = pkgs.stdenv.mkDerivation {
    inherit pname;
    version = pkgs.crystal-forge.default.version;
    src = pkgs.crystal-forge.default;
    installPhase = ''
      mkdir -p $out/bin
      cp ${pkgs.crystal-forge.default}/bin/${pname} $out/bin/${pname}
    '';
  };
in
  crystal-forge-server
