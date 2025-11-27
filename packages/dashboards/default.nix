{
  lib,
  pkgs,
  ...
}:
pkgs.stdenv.mkDerivation {
  pname = "crystal-forge-dashboard";
  version = "0.1.0";

  src = ./.;

  nativeBuildInputs = [pkgs.jq];

  dontConfigure = true;

  installPhase = ''
    runHook preInstall

    mkdir -p $out/dashboards
    cp crystal-forge-dashboard.json $out/dashboards/crystal-forge-dashboard.json
    runHook postInstall
  '';

  passthru = {
    # Convenience accessor for a specific dashboard file
    dashboardPath = "${placeholder "out"}/dashboards/crystal-forge-dashboard.json";
    # Directory containing all dashboards - use this for Grafana provisioning
    dashboardsDir = "${placeholder "out"}/dashboards";
  };

  meta = with lib; {
    description = "Crystal Forge monitoring dashboard for Grafana";
    license = licenses.agpl3Only;
    platforms = platforms.all;
  };
}
