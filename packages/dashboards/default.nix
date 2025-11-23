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

  buildPhase = ''
    runHook preBuild
    # TODO: This isnt needed any more..
    # Process the dashboard to make it generic and reusable
    jq '
      # Set uid to null so Grafana auto-generates it
      .uid = null |

      # Update the datasource input definition
      .["__inputs"][0].name = "DS_CRYSTAL_FORGE_POSTGRES" |
      .["__inputs"][0].label = "Crystal Forge PostgreSQL" |

      # Replace all datasource UID references throughout the dashboard
      walk(
        if type == "object" and has("uid") and .uid != null then
          if (.uid | type == "string") and (.uid | test("DS_RECKLESS-POSTGRES-CRYSTAL-FORGE|DS_.*POSTGRES.*CRYSTAL.*FORGE")) then
            .uid = "''${DS_CRYSTAL_FORGE_POSTGRES}"
          else
            .
          end
        else
          .
        end
      )
    ' crystal-forge-dashboard.json > crystal-forge-dashboard-processed.json

    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall

    mkdir -p $out/dashboards
    cp crystal-forge-dashboard-processed.json $out/dashboards/crystal-forge-dashboard.json

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
