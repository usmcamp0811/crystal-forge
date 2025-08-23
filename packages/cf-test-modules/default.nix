{
  pkgs,
  lib,
  ...
}: let
  # Build the Python package
  cfTestModules = pkgs.python3Packages.buildPythonPackage rec {
    pname = "cf_test_modules";
    version = "1.0.0";
    format = "pyproject";
    src = ./.;
    nativeBuildInputs = with pkgs.python3Packages; [
      setuptools
      wheel
    ];
    propagatedBuildInputs = with pkgs.python3Packages; [
      pytest
    ];
    pythonImportsCheck = ["cf_test_modules"];
    meta = with lib; {
      description = "Modular test components for Crystal Forge integration testing";
    };
  };

  # Create the test runner script
  testRunner = pkgs.writeShellApplication {
    name = "cf-devshell-test-runner";
    runtimeInputs = with pkgs; [
      python3
      postgresql # for pg_isready
      curl # for server health checks
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

      # Add cf_test_modules to Python path
      export PYTHONPATH="${cfTestModules}/${pkgs.python3.sitePackages}:''${PYTHONPATH:-}"

      # Run the test runner
      exec python3 "${cfTestModules}/${pkgs.python3.sitePackages}/cf_test_modules/test_runner.py" "$@"
    '';
  };

  runTests = pkgs.writeShellApplication {
    name = "run-cf-view-tests";
    runtimeInputs = [testRunner];
    text = ''
      echo "🧪 Crystal Forge DevShell View Tests"
      echo "====================================="
      echo "Database: ''${DB_USER:-crystal_forge}@''${DB_HOST:-127.0.0.1}:''${DB_PORT:-3042}/''${DB_NAME:-crystal_forge}"
      echo "Server: ''${CF_SERVER_HOST:-127.0.0.1}:''${CF_SERVER_PORT:-3445}"
      echo ""

      # Check if process-compose is likely running
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

      # Check if Crystal Forge server is running (optional) - FIXED URL
      if curl -s -f "http://''${CF_SERVER_HOST:-127.0.0.1}:''${CF_SERVER_PORT:-3445}/status" >/dev/null 2>&1; then
        echo "✅ Crystal Forge server connection OK"
      else
        echo "⚠️  Cannot reach Crystal Forge server (tests will continue but some may fail)"
      fi

      echo ""
      echo "🏃 Running view tests..."
      echo ""

      cf-devshell-test-runner "$@"
    '';
  };
in
  cfTestModules
  // {
    inherit testRunner runTests;
  }
