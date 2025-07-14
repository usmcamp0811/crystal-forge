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

      echo "Debug info:"
      echo "  DB_HOST: $DB_HOST"
      echo "  DB_PORT: $DB_PORT"
      echo "  DB_NAME: $DB_NAME"
      echo "  DB_USER: $DB_USER"
      echo "  DB_PASSWORD: [''${#DB_PASSWORD} chars]"
      echo "  JOB_DIR: $JOB_DIR"

      # Set password if provided
      if [[ -n "$DB_PASSWORD" ]]; then
        export PGPASSWORD="$DB_PASSWORD"
      fi

      echo "Connecting to database: $DB_NAME on $DB_HOST:$DB_PORT as user $DB_USER"

      # Test connection with verbose output
      echo "Testing connection..."
      if ! psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -c '\q'; then
        echo "Failed to connect to database. Detailed error above."
        echo "Attempting to check if database is running..."
        if ! pg_isready -h "$DB_HOST" -p "$DB_PORT"; then
          echo "Database server is not ready"
        else
          echo "Database server is ready, but connection failed"
        fi
        exit 1
      fi

      echo "Connection test successful"

      # Check if job directory exists
      if [[ ! -d "$JOB_DIR" ]]; then
        echo "Job directory does not exist: $JOB_DIR"
        exit 1
      fi

      # List available SQL files
      echo "Available SQL files:"
      find "$JOB_DIR" -type f -name '*.sql' | sort || echo "No SQL files found"

      # Run all SQL jobs
      while IFS= read -r -d "" sql_file; do
        echo "Running job: $(basename "$sql_file")"
        if ! psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -f "$sql_file"; then
          echo "Failed to run job: $(basename "$sql_file")"
          exit 1
        fi
      done < <(find "$JOB_DIR" -type f -name '*.sql' -print0 | sort -z)

      echo "All jobs completed successfully"
    '';
  }
