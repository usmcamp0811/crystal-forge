{
  lib,
  pkgs,
  ...
}: let
  pname = "agent";
  crystal-forge-agent = pkgs.stdenv.mkDerivation {
    inherit pname;
    version = pkgs.crystal-forge.default.version;
    src = pkgs.crystal-forge.default;
    installPhase = ''
      mkdir -p $out/bin
      cp ${pkgs.crystal-forge.default}/bin/${pname} $out/bin/${pname}
      cp ${pkgs.crystal-forge.default}/bin/cf-keygen $out/bin/cf-keygen
    '';
  };
in
  crystal-forge-agent
  // {
    cf-keygen = pkgs.writeShellApplication {
      name = "cf-keygen";
      text = "${pkgs.crystal-forge.default}/bin/cf-keygen \"$@\"";
    };
    test-agent = pkgs.writeShellApplication {
      name = "test-agent";
      text = "${pkgs.crystal-forge.default}/bin/test-agent \"$@\"";
    };
  }
