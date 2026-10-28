{pkgs, lib, ...}: let
  python = pkgs.python3.withPackages (ps: [ps.psycopg]);
in pkgs.stdenvNoCC.mkDerivation {
  pname = "cf-attic-metrics";
  version = "0.1.0";
  src = lib.cleanSource ./.;
  nativeBuildInputs = [pkgs.makeWrapper];
  dontBuild = true;
  doCheck = true;
  checkPhase = ''
    PYTHONDONTWRITEBYTECODE=1 ${python}/bin/python -m unittest -v test_attic_metrics
  '';
  installPhase = ''
    install -Dm644 attic_metrics.py $out/libexec/attic_metrics.py
    makeWrapper ${python}/bin/python $out/bin/cf-attic-metrics \
      --add-flags "$out/libexec/attic_metrics.py"
  '';
  meta = {
    description = "Optional authenticated logical inventory adapter for Attic";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
    mainProgram = "cf-attic-metrics";
  };
}
