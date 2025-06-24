# ./services/postgres.nix
{
  config,
  pkgs,
  lib,
  ...
}: {
  options.services.pg = {
    enable = lib.mkEnableOption "Enable local postgres";
  };

  config = let
    cfg = config.services.pg;
  in
    lib.mkIf cfg.enable {
      settings.processes.postgres = {
        command = "${pkgs.postgresql}/bin/postgres -D ./pgdata -k /tmp";
        readiness_probe.exec.command = "${pkgs.pg_isready}/bin/pg_isready";
      };
    };
}
