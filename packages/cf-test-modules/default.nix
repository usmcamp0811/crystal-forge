{
  pkgs,
  lib,
  ...
}: let
  # Build the Python package using the new simple pytest-based framework
  cfTest = pkgs.python3Packages.buildPythonPackage rec {
    pname = "cf-test";
    version = "1.0.0";
    format = "pyproject";
    src = ./.;

    nativeBuildInputs = with pkgs.python3Packages; [
      setuptools
      wheel
    ];

    propagatedBuildInputs = with pkgs.python3Packages; [
      pytest
      pytest-html # For HTML reports
      pytest-xdist # For parallel test execution
      psycopg2 # PostgreSQL adapter (maps to psycopg2-binary in PyPI)
    ];

    pythonImportsCheck = ["cf_test"];

    meta = with lib; {
      description = "Simple pytest-based testing framework for Crystal Forge";
      license = licenses.mit;
    };
  };

  # Create the test runner script for devshell
  testRunner = pkgs.writeShellApplication {
    name = "cf-test-runner";
    runtimeInputs = with pkgs; [
      python3
      postgresql # for pg_isready and psql
      curl # for server health checks
      cfTest # Include our test package
    ];
    text = ''
      # Set default environment variables for devshell testing
      export DB_HOST="''${DB_HOST:-127.0.0.1}"
      export DB_PORT="''${DB_PORT:-3042}"
      export DB_USER="''${DB_USER:-crystal_forge}"
      export DB_PASSWORD="''${DB_PASSWORD:-password}"
      export DB_NAME="''${DB_NAME:-crystal_forge}"
      export CF_SERVER_HOST="''${CF_SERVER_HOST:-127.0.0.1}"
      export CF_SERVER_PORT="''${CF_SERVER_PORT:-3445}"
      export HOSTNAME="''${HOSTNAME:-$(hostname -s)}"

      # Run cf-test with all arguments passed through
      exec cf-test "$@"
    '';
  };

  # Convenience script with health checks
  runTests = pkgs.writeShellApplication {
    name = "run-cf-tests";
    runtimeInputs = [testRunner];
    text = ''
      echo "🧪 Crystal Forge Tests"
      echo "===================="
      echo "Database: ''${DB_USER:-crystal_forge}@''${DB_HOST:-127.0.0.1}:''${DB_PORT:-3042}/''${DB_NAME:-crystal_forge}"
      echo "Server: ''${CF_SERVER_HOST:-127.0.0.1}:''${CF_SERVER_PORT:-3445}"
      echo ""

      # Check if PostgreSQL is available
      if ! pg_isready -h "''${DB_HOST:-127.0.0.1}" -p "''${DB_PORT:-3042}" -U "''${DB_USER:-crystal_forge}" 2>/dev/null; then
        echo "❌ Cannot connect to PostgreSQL at ''${DB_HOST:-127.0.0.1}:''${DB_PORT:-3042}"
        echo ""
        echo "Please start process-compose first:"
        echo "  nix run .#cf-dev"
        echo ""
        echo "Then run this script in another terminal."
        exit 1
      fi
      echo "✅ PostgreSQL connection OK"

      # Check if Crystal Forge server is running (optional)
      if curl -s -f "http://''${CF_SERVER_HOST:-127.0.0.1}:''${CF_SERVER_PORT:-3445}/health" >/dev/null 2>&1 || \
         curl -s -f "http://''${CF_SERVER_HOST:-127.0.0.1}:''${CF_SERVER_PORT:-3445}/status" >/dev/null 2>&1; then
        echo "✅ Crystal Forge server connection OK"
      else
        echo "⚠️  Cannot reach Crystal Forge server (tests will continue but some may fail)"
      fi

      echo ""
      echo "🏃 Running tests..."
      echo ""

      # Run the tests with nice defaults
      if [ $# -eq 0 ]; then
        # No arguments - run smoke tests first, then all tests
        echo "Running smoke tests first..."
        cf-test-runner -m smoke --tb=line || {
          echo "❌ Smoke tests failed, stopping"
          exit 1
        }
        echo ""
        echo "✅ Smoke tests passed, running all tests..."
        cf-test-runner --tb=short --html=test-results/report.html
      else
        # Pass through all arguments
        cf-test-runner "$@"
      fi
    '';
  };

  # Quick test commands for different scenarios
  smokeTests = pkgs.writeShellApplication {
    name = "cf-smoke-tests";
    runtimeInputs = [testRunner];
    text = ''
      echo "🚀 Running Crystal Forge smoke tests..."
      cf-test-runner -m smoke --tb=line
    '';
  };

  databaseTests = pkgs.writeShellApplication {
    name = "cf-database-tests";
    runtimeInputs = [testRunner];
    text = ''
      echo "🗄️ Running Crystal Forge database tests..."
      cf-test-runner -m database --tb=short
    '';
  };

  viewTests = pkgs.writeShellApplication {
    name = "cf-view-tests";
    runtimeInputs = [testRunner];
    text = ''
      echo "👁️ Running Crystal Forge view tests..."
      cf-test-runner -m views --tb=short
    '';
  };

  integrationTests = pkgs.writeShellApplication {
    name = "cf-integration-tests";
    runtimeInputs = [testRunner];
    text = ''
      echo "🔗 Running Crystal Forge integration tests..."
      cf-test-runner -m integration --maxfail=3 --tb=short
    '';
  };
in
  cfTest
  // {
    # Export the test package as the main result
    inherit testRunner runTests;

    # Export convenience commands
    inherit smokeTests databaseTests viewTests integrationTests;
  }
