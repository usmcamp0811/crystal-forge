{
  pkgs,
  config,
  ...
}: let
  sql-jobs = ./jobs/.;

  # Create a derivation that bundles the script and jobs together
  run-postgres-jobs-with-jobs = pkgs.stdenv.mkDerivation {
    pname = "run-postgres-jobs";
    version = "0.1.0";

    src = ./.;

    dontBuild = true;

    installPhase = ''
      # Create directories
      mkdir -p $out/bin
      mkdir -p $out/jobs

      # Copy the shell application binary
      cp ${run-postgres-jobs-script}/bin/run-postgres-jobs $out/bin/

      # Copy SQL job files
      cp -r ${sql-jobs}/*.sql $out/jobs/
    '';

    meta = with pkgs.lib; {
      description = "Crystal Forge postgres jobs runner with bundled SQL files";
      license = licenses.mit;
      platforms = platforms.unix;
    };
  };

  # Keep your original shell application
  run-postgres-jobs-script = pkgs.writeShellApplication {
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
  };
in
  run-postgres-jobs-with-jobs
