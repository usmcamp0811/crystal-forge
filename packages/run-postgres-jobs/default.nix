{
  pkgs,
  config,
  ...
}: let
  sql-jobs = ./jobs/.;
in
  pkgs.writeShellApplication {
    name = "run-postgres-jobs";
    runtimeInputs = [pkgs.postgresql];
    text = ''
      set -euo pipefail

      # Database connection parameters with defaults
      DB_HOST="''${DB_HOST:-localhost}"
      DB_PORT="''${DB_PORT:-5432}"
      DB_NAME="''${DB_NAME:-crystal_forge}"
      DB_USER="''${DB_USER:-crystal_forge}"
      DB_PASSWORD="''${DB_PASSWORD:-}"
      JOB_DIR="''${JOB_DIR:-${toString sql-jobs}}"

      # Build connection string
      CONNECTION_OPTS="-h $DB_HOST -p $DB_PORT -U $DB_USER -d $DB_NAME"

      # Set password if provided
      if [[ -n "$DB_PASSWORD" ]]; then
        export PGPASSWORD="$DB_PASSWORD"
      fi

      echo "🔗 Connecting to database: $DB_NAME on $DB_HOST:$DB_PORT as user $DB_USER"

      # Test connection first
      if ! psql $CONNECTION_OPTS -c '\q' 2>/dev/null; then
        echo "❌ Failed to connect to database. Check your connection parameters."
        exit 1
      fi

      # Run all SQL jobs
      for sql_file in $(find "$JOB_DIR" -type f -name '*.sql' | sort); do
        echo "🔧 Running job: $(basename "$sql_file")"
        if ! psql $CONNECTION_OPTS -f "$sql_file"; then
          echo "❌ Failed to run job: $(basename "$sql_file")"
          exit 1
        fi
      done

      echo "✅ All jobs completed successfully"
    '';
  }
