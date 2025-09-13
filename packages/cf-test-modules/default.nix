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
      postgresql
      curl
      cfTest
    ];
    text = ''
      set -euo pipefail

      # Prefer local tree if present; fall back to packaged copy in /nix/store
      if [ -d "$PWD/cf_test/tests" ]; then
        TESTS_DIR="$PWD/cf_test/tests"
        export PYTHONPATH="$PWD:$PWD/cf_test:$PYTHONPATH"
      else
        TESTS_DIR='${cfTest}/${pkgs.python3.sitePackages}/cf_test/tests'
        export PYTHONPATH="${cfTest}/${pkgs.python3.sitePackages}:$PYTHONPATH"
      fi
      export PYTHONDONTWRITEBYTECODE=1

      export DB_HOST="''${DB_HOST:-127.0.0.1}"
      export DB_PORT="''${DB_PORT:-5432}"
      export DB_USER="''${DB_USER:-crystal_forge}"
      export DB_PASSWORD="''${DB_PASSWORD:-password}"
      export DB_NAME="''${DB_NAME:-crystal_forge}"
      export CF_SERVER_HOST="''${CF_SERVER_HOST:-127.0.0.1}"
      export CF_SERVER_PORT="''${CF_SERVER_PORT:-3000}"
      export HOSTNAME="''${HOSTNAME:-$(hostname -s)}"

      # Always pass the tests directory to pytest (via cf-test entrypoint)
      exec cf-test "$@" "$TESTS_DIR"
    '';
  };

  # Convenience script with health checks
  #TODO: Clean up order marks and things
  runTests = pkgs.writeShellApplication {
    name = "run-cf-tests";
    runtimeInputs = [testRunner cfTest];
    text = ''
      set +e
      set -o pipefail

      if [ -d "$PWD/cf_test/tests" ]; then
        TESTS_DIR="$PWD/cf_test/tests"
        export PYTHONPATH="$PWD:$PWD/cf_test:$PYTHONPATH"
      else
        TESTS_DIR='${cfTest}/${pkgs.python3.sitePackages}/cf_test/tests'
        export PYTHONPATH="${cfTest}/${pkgs.python3.sitePackages}:$PYTHONPATH"
      fi
      export PYTHONDONTWRITEBYTECODE=1

      mkdir -p test-results

      CONTINUE=0
      CLEAN_ARGS=()
      for a in "$@"; do
        if [ "$a" = "--continue-on-fail" ]; then
          CONTINUE=1
        else
          CLEAN_ARGS+=("$a")
        fi
      done

      echo "🧪 Crystal Forge Tests"
      echo "===================="
      echo "Database: ''${DB_USER:-crystal_forge}@''${DB_HOST:-127.0.0.1}:''${DB_PORT:-5432}/''${DB_NAME:-crystal_forge}"
      echo "Server: ''${CF_SERVER_HOST:-127.0.0.1}:''${CF_SERVER_PORT:-3000}"
      echo ""

      if ! pg_isready -h "''${DB_HOST:-127.0.0.1}" -p "''${DB_PORT:-5432}" -U "''${DB_USER:-crystal_forge}" >/dev/null 2>&1; then
        echo "❌ Cannot connect to PostgreSQL at ''${DB_HOST:-127.0.0.1}:''${DB_PORT:-5432}"
        echo "  nix run .#cf-dev"
        exit 1
      fi
      echo "✅ PostgreSQL connection OK"

      if curl -s -f "http://''${CF_SERVER_HOST:-127.0.0.1}:''${CF_SERVER_PORT:-3000}/health" >/dev/null 2>&1 || \
         curl -s -f "http://''${CF_SERVER_HOST:-127.0.0.1}:''${CF_SERVER_PORT:-3000}/status" >/dev/null 2>&1; then
        echo "✅ Crystal Forge server connection OK"
      else
        echo "⚠️  Cannot reach Crystal Forge server (continuing)"
      fi
      echo ""
      echo "🏃 Running tests..."
      echo ""

      if [ "''${#CLEAN_ARGS[@]}" -eq 0 ]; then
        echo "Running smoke tests first..."
        cf-test-runner -m smoke --tb=line "$TESTS_DIR"
        smoke_status=$?

        if [ "$CONTINUE" -ne 1 ] && [ "$smoke_status" -ne 0 ]; then
          echo "❌ Smoke tests failed, stopping (use --continue-on-fail to run full suite anyway)"
          exit $smoke_status
        fi

        echo ""
        echo "➡️  Running full suite..."
        if [ "$CONTINUE" -eq 1 ]; then
          cf-test-runner --tb=short --html=test-results/report.html --continue-on-fail "$TESTS_DIR"
        else
          cf-test-runner --tb=short --html=test-results/report.html "$TESTS_DIR"
        fi
        suite_status=$?

        [ "$smoke_status" -ne 0 ] && exit 1
        [ "$suite_status" -ne 0 ] && exit 1
        exit 0
      else
        has_positional=0
        for a in "''${CLEAN_ARGS[@]}"; do
          case "$a" in -*) ;; *) has_positional=1; break;; esac
        done
        if [ "$has_positional" -eq 0 ]; then
          CLEAN_ARGS+=("$TESTS_DIR")
        fi

        if [ "$CONTINUE" -eq 1 ]; then
          cf-test-runner --continue-on-fail "''${CLEAN_ARGS[@]}"
        else
          cf-test-runner "''${CLEAN_ARGS[@]}"
        fi
        exit $?
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

  python = pkgs.python3.withPackages (ps: [
    ps.pytest
    ps.pytest-html
    ps.pytest-xdist
    ps.psycopg2
    cfTest
  ]);
  scenarioRunner = pkgs.writeShellApplication {
    name = "cf-scenarios";
    runtimeInputs = with pkgs; [postgresql cfTest] ++ [python];
    text = ''
      set -euo pipefail

      if [ -d "$PWD/cf_test" ]; then
        export PYTHONPATH="$PWD:$PWD/cf_test:$PYTHONPATH"
      else
        export PYTHONPATH="${cfTest}/${pkgs.python3.sitePackages}:$PYTHONPATH"
      fi
      export PYTHONDONTWRITEBYTECODE=1

      export DB_HOST="''${DB_HOST:-127.0.0.1}"
      export DB_PORT="''${DB_PORT:-5432}"
      export DB_USER="''${DB_USER:-crystal_forge}"
      export DB_PASSWORD="''${DB_PASSWORD:-password}"
      export DB_NAME="''${DB_NAME:-crystal_forge}"
      export CF_SERVER_HOST="''${CF_SERVER_HOST:-127.0.0.1}"
      export CF_SERVER_PORT="''${CF_SERVER_PORT:-3000}"

      exec python -m cf_test.scenarios "$@"
    '';
  };

  # in your lib where `pkgs`, `lib.crystal-forge.testFlake`, and `pkgs.path` are in scope
  derivation-paths =
    pkgs.runCommand "derivation-paths.json" {
      nativeBuildInputs = [pkgs.nix pkgs.git];
      testFlakeSource = lib.crystal-forge.testFlake;
      NIX_CONFIG = "experimental-features = nix-command flakes";
      NIXPKGS_PATH = pkgs.path;
    } ''
          set -euo pipefail

          export HOME=$TMPDIR
          export NIX_USER_PROFILE_DIR=$TMPDIR/profiles
          export NIX_PROFILES="$NIX_USER_PROFILE_DIR/profile"
          mkdir -p "$NIX_USER_PROFILE_DIR"

          flake="git+file://$testFlakeSource?ref=main"

          cf_test_sys_drv=$(
            nix eval --impure --raw \
              --override-input nixpkgs "path:$NIXPKGS_PATH" \
              "$flake#nixosConfigurations.cf-test-sys.config.system.build.toplevel.drvPath"
          )

          test_agent_drv=$(
            nix eval --impure --raw \
              --override-input nixpkgs "path:$NIXPKGS_PATH" \
              "$flake#nixosConfigurations.test-agent.config.system.build.toplevel.drvPath"
          )

          cat >"$out" <<EOF
      {
        "cf-test-sys": {
          "derivation_path": "$cf_test_sys_drv",
          "derivation_name": "cf-test-sys",
          "derivation_type": "nixos"
        },
        "test-agent": {
          "derivation_path": "$test_agent_drv",
          "derivation_name": "test-agent",
          "derivation_type": "nixos"
        }
      }
      EOF
    '';
in
  cfTest
  // {
    # Export the test package as the main result
    inherit
      testRunner
      runTests
      smokeTests
      databaseTests
      viewTests
      integrationTests
      scenarioRunner
      derivation-paths
      ;
  }
