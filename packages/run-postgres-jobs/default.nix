{
  pkgs,
  config,
  ...
}: let
  postgres_pkg = config.services.postgresql.package;
  sql-jobs = ./jobs/.;
in
  pkgs.writeShellApplication {
    name = "run-postgres-jobs";
    runtimeInputs = [postgres_pkg];
    text = ''
      set -euo pipefail

      DB_NAME="''${DB_NAME:-crystal_forge}"
      DB_USER="''${DB_USER:-crystal_forge}"
      JOB_DIR="''${JOB_DIR:-${toString sql-jobs}}"

      for sql_file in $(find "$JOB_DIR" -type f -name '*.sql' | sort); do
        echo "🔧 Running job: $(basename "$sql_file")"
        psql -U "$DB_USER" -d "$DB_NAME" -f "$sql_file"
      done
    '';
  }
