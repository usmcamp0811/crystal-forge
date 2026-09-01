# Shared Web UI Check Constructor
#
# Every stable Web UI check imports this constructor. The constructor keeps the
# production server, browser, fixture, and test inputs identical while selecting
# one independently reproducible responsibility through the parameters below.
# The outer derivation is the logical gate. Its `evidence` passthru points to the
# VM derivation, which succeeds after recording browser failures so CI can copy
# failed-step evidence without rerunning the VM. Infrastructure failures still
# fail the evidence derivation.
#
# Manifest-driven Playwright verification runs against a real Crystal Forge
# server (PostgreSQL + gitserver), with:
# - Explicit build verification (served index, JS loader, and packaged WASM)
# - Semantic assertions + screenshots per coverage-manifest.json step
# - Design-parity visual comparison: Dioxus screenshots vs rendered design
#   example targets (docs/design/CrystalForge, vendored offline, non-blocking)
# - OSCAL and SARIF export validation against vendored schemas
#
# Coverage is defined in ./coverage-manifest.json — the check fails if the
# steps in tests/integration-test.js drift from the manifest.
#
# Optional legacy "mega" phases (Attic/S3 cache + builder pytest suites) are
# opt-in via CF_WEB_UI_RUN_MEGA_PHASES=1 (interactive runs only — the env var
# cannot cross the Nix build sandbox). Their VMs are only booted when enabled.
#
# Note: OIDC tests remain in the separate integration check.
#
{ lib
, pkgs
, inputs
, checkName ? "web-ui"
, testProfile ? "compatibility"
, testSteps ? builtins.getEnv "CF_UI_TEST_STEPS"
, runAssetVerification ? true
, runBrowserSemanticValidation ? true
, runExportValidation ? false
, runDesignParity ? false
, gateBrowserValidation ? true
, blocking ? true
, updateVisualBaselines ? builtins.getEnv "CF_UI_UPDATE_BASELINES" == "1"
, playwrightProcessTimeout ? 900
, playwrightResultTimeout ? 960
, ...
}:
let
  testDir = ./tests;
  coverageManifest = ./coverage-manifest.json;
  checkGroups = ./check-groups.json;
  baselinesDir = ./baselines;
  designParityDir = ./design-parity;
  gateVerdictChecker = ../../ci/check-web-ui-verdict.js;
  CF_TEST_SERVER_PORT = 3000;
  debugProcessTimeout = builtins.getEnv "CF_UI_PROCESS_TIMEOUT";
  effectivePlaywrightProcessTimeout =
    if debugProcessTimeout == ""
    then playwrightProcessTimeout
    else builtins.fromJSON debugProcessTimeout;

  # ── Design-parity harness (non-blocking) ────────────────────────────────────
  # Vendor the design example's CDN dependencies so the tracked design gold
  # standard (docs/design/CrystalForge) renders fully offline inside the check
  # VM. sha256 hashes from nix-prefetch-url.
  reactUmd = pkgs.fetchurl {
    url = "https://unpkg.com/react@18.3.1/umd/react.development.js";
    sha256 = "0zsfq9pj3pbpiz9p6k6qflwd33s24kwflbdjxqn8pvdhdkpqyd18";
  };
  reactDomUmd = pkgs.fetchurl {
    url = "https://unpkg.com/react-dom@18.3.1/umd/react-dom.development.js";
    sha256 = "1r09hyz12n03w6fvcnv93ri0mv16wljgkpq4laqqpnrrkig4l17r";
  };
  babelStandalone = pkgs.fetchurl {
    url = "https://unpkg.com/@babel/standalone@7.29.0/babel.min.js";
    sha256 = "186f1mfjlcs49p0j0hss1m9cxpbpw9a12imli7kmr48953iaj8r6";
  };

  designExampleSrc = inputs.self + "/docs/design/CrystalForge";

  # Offline copy of the design example with the three CDN <script> tags rewritten
  # to the vendored local files so Playwright can render it with no network.
  designExampleOffline = pkgs.runCommand "cf-design-example-offline" { } ''
    mkdir -p $out/vendor
    cp -r ${designExampleSrc}/. $out/
    chmod -R u+w $out
    cp ${reactUmd} $out/vendor/react.development.js
    cp ${reactDomUmd} $out/vendor/react-dom.development.js
    cp ${babelStandalone} $out/vendor/babel.min.js

    # Rewrite CDN script srcs to vendored paths and drop SRI/crossorigin so the
    # local files load without integrity/CORS checks.
    ${pkgs.gnused}/bin/sed -i -E \
      -e 's#src="https://unpkg.com/react@[^"]*"#src="vendor/react.development.js"#' \
      -e 's#src="https://unpkg.com/react-dom@[^"]*"#src="vendor/react-dom.development.js"#' \
      -e 's#src="https://unpkg.com/@babel/standalone@[^"]*"#src="vendor/babel.min.js"#' \
      -e 's# integrity="[^"]*"##g' \
      -e 's# crossorigin="anonymous"##g' \
      $out/crystal-forge.html
  '';

  # Explicit web-ui build verification (AC from TASK-8.12, preserved here):
  # index.html is served, it references a JS loader, the loader is served, and
  # the referenced packaged WASM output has a valid wasm magic header.
  verifyWebUiAssets = pkgs.writeShellScript "verify-web-ui-assets" ''
    set -euo pipefail
    base="$1"
    ui_dist="$2"

    normalize_asset_path() {
      local p="$1"
      p="''${p#./}"
      p="''${p#/./}"
      p="''${p#/}"
      printf '%s' "$p"
    }

    index=$(curl -sf "$base/") || { echo "FAIL: index.html not served"; exit 1; }

    js_path=$(printf '%s' "$index" | grep -oE '(src|href)="[^"]+\.js"' | head -1 | sed -E 's/^(src|href)="//; s/"$//')
    [ -n "$js_path" ] || { echo "FAIL: no JS loader reference in index.html"; exit 1; }
    case "$js_path" in
      http*) js_url="$js_path" ;;
      *) js_url="$base/$(normalize_asset_path "$js_path")" ;;
    esac

    js=$(curl -sf "$js_url") || { echo "FAIL: JS loader $js_url not served"; exit 1; }

    wasm_ref=$(printf '%s' "$js" | grep -oE '"[^"]*\.wasm"' | head -1 | tr -d '"')
    if [ -z "$wasm_ref" ]; then
      wasm_ref=$(printf '%s' "$index" | grep -oE '"[^"]*\.wasm"' | head -1 | tr -d '"')
    fi
    [ -n "$wasm_ref" ] || { echo "FAIL: no .wasm reference found in loader or index"; exit 1; }

    wasm_name=$(basename "$(normalize_asset_path "$wasm_ref")")
    wasm_path=$(find "$ui_dist" -type f -name "$wasm_name" | head -1)
    if [ -z "$wasm_path" ]; then
      wasm_path=$(find "$ui_dist" -type f -name '*.wasm' | head -1)
    fi
    [ -n "$wasm_path" ] || { echo "FAIL: no wasm output found under $ui_dist"; exit 1; }

    magic=$(head -c4 "$wasm_path" | od -An -tx1 | tr -d ' \n')
    [ "$magic" = "0061736d" ] || { echo "FAIL: wasm output $wasm_path has invalid magic ($magic)"; exit 1; }

    echo "web-ui build verification OK: index served, loader $js_path, wasm output $wasm_path"
  '';

  # Use fixed test keys to avoid cf-keygen CI flakiness
  # Keys embedded directly to avoid path resolution issues in Nix build context
  keyPath = pkgs.runCommand "agent.key" { } ''
    mkdir -p $out
    echo "+/GIbrjuyb3Hf2es5w+vWSlDUhEsAIojiyyfgskC7QA=" > $out/agent.key
  '';
  pubPath = pkgs.runCommand "agent.pub" { } ''
    mkdir -p $out
    echo "DpOiy7W+DqZEg3KR0fvP5Q8k4FR4K1NB+qyYQLxhnFc=" > $out/agent.pub
  '';
  derivation-paths = lib.crystal-forge.derivation-paths pkgs;

  # Components this check actually runs.
  #
  # This check enables the server, the builder, and the agent, so all three
  # components are required. They are listed explicitly rather than through an
  # aggregate so the closure states what the VM runs.
  #
  # COMPATIBILITY: Every partition uses the production embedded-UI server, not
  # the core build used by API-only checks. The `web-ui` wrapper also enables
  # `verifyWebUiAssets` and proves that this server serves the shipped WASM in
  # real Chromium. Do not replace this binding in a wrapper. A replacement
  # would break shared derivation identity and could remove the production
  # packaging guarantee.
  cfServer = pkgs.crystal-forge.default.cf-server-drv;
  cfAgent = pkgs.crystal-forge.default.cf-agent-drv;
  cfBuilder = pkgs.crystal-forge.default.cf-builder-drv;

  systemBuildClosure = pkgs.closureInfo {
    rootPaths = [
      inputs.self.nixosConfigurations.cf-test-sys.config.system.build.toplevel
      cfServer
      cfAgent
      cfBuilder
      pkgs.path
    ] ++ lib.crystal-forge.prefetchedPaths;
  };
  evidence = pkgs.testers.runNixOSTest {
    name = "crystal-forge-${checkName}-evidence";

  skipLint = true;
  skipTypeCheck = true;

  nodes = {
    # Git server for flake testing
    gitserver = lib.crystal-forge.makeGitServerNode {
      inherit pkgs systemBuildClosure;
      port = 8080;
    };

    # Attic binary cache
    atticCache = lib.crystal-forge.makeAtticCacheNode {
      inherit lib pkgs;
      port = 8080;
      jwtSecretB64 = "dGVzdCBzZWNyZXQgZm9yIGF0dGljZA==";
    };

    # S3-compatible cache (Garage)
    s3Cache = lib.crystal-forge.makeS3CacheNode {
      inherit lib pkgs;
      port = 3900;
      bucketName = "nix-cache";
    };

    # Main Crystal Forge server with all services enabled
    machine = {
      imports = [ inputs.self.nixosModules.crystal-forge ];

      virtualisation.memorySize = 20480; # 20GB for everything
      virtualisation.cores = 4;
      virtualisation.diskSize = 40960;
      virtualisation.writableStore = true;
      virtualisation.additionalPaths = [
        systemBuildClosure
        inputs.self.nixosConfigurations.cf-test-sys.config.system.build.toplevel.drvPath
        inputs.nixpkgs.outPath
      ] ++ lib.crystal-forge.prefetchedPaths;

      environment.systemPackages = [
        pkgs.chromium
        pkgs.nodejs
        pkgs.playwright-test
        pkgs.curl
        pkgs.jq
        pkgs.git
        # ImageMagick provides `compare`/`identify` for screenshot baseline diffs
        pkgs.imagemagick
        cfServer
        cfAgent
        cfBuilder
        pkgs.crystal-forge.default.migrate
        pkgs.crystal-forge.cf-test-suite.runTests
        pkgs.crystal-forge.cf-test-suite.testRunner
        # Python with jsonschema+regex for OSCAL and SARIF export validation
        (pkgs.python3.withPackages (p: [ p.jsonschema p.regex ]))
        # Vendored NIST OSCAL 1.1.2 schemas for OSCAL export validation
        pkgs.crystal-forge.oscal-1-1-2-schemas
        # Vendored OASIS SARIF 2.1.0 Errata 01 schema for SARIF export validation
        pkgs.crystal-forge.sarif-2-1-0-schema
      ];

      environment.variables = {
        NODE_PATH = "${pkgs.playwright-test}/lib/node_modules";
        PLAYWRIGHT_BROWSERS_PATH = "${pkgs.playwright-driver.browsers}";
        TMPDIR = "/tmp";
        TMP = "/tmp";
        TEMP = "/tmp";
      };

      environment.etc = {
        "server.key".source = "${keyPath}/agent.key";
        "server.pub".source = "${pubPath}/agent.pub";
      };

      networking.firewall.allowedTCPPorts = [ CF_TEST_SERVER_PORT 5432 ];

      systemd.tmpfiles.rules = [
        "d /var/lib/crystal-forge 0755 crystal-forge crystal-forge -"
        "d /var/lib/crystal-forge/.cache 0755 crystal-forge crystal-forge -"
        "d /var/lib/crystal-forge/.cache/nix 0755 crystal-forge crystal-forge -"
        "Z /var/lib/crystal-forge/.cache/nix - crystal-forge crystal-forge -"
      ];

      # PostgreSQL
      services.postgresql = {
        enable = true;
        settings = {
          "listen_addresses" = lib.mkForce "*";
          "fsync" = "off";
          "synchronous_commit" = "off";
          "full_page_writes" = "off";
          "max_wal_size" = "64MB";
          "min_wal_size" = "32MB";
        };
        authentication = lib.concatStringsSep "\n" [
          "local   all   postgres   trust"
          "local   all   all        peer"
          "host    all   all 127.0.0.1/32 trust"
          "host    all   all ::1/128      trust"
          "host    all   all 10.0.2.2/32  trust"
        ];
      };

      # Crystal Forge - starts with local auth, will switch to OIDC during test
      services.crystal-forge = {
        enable = true;
        local-database = true;
        # Builder startup tests assert INFO-level startup logs.
        # Keep INFO here so those assertions remain observable in the mega check.
        log_level = "info";

        client = {
          enable = true;
          package = cfAgent;
          server_host = "localhost";
          server_port = CF_TEST_SERVER_PORT;
          private_key = "/etc/server.key";
        };

        database = {
          host = "localhost";
          user = "crystal_forge";
          name = "crystal_forge";
          port = 5432;
        };

        server = {
          enable = true;
          # Production embedded-UI build. See the cfServer binding above.
          package = cfServer;
          port = CF_TEST_SERVER_PORT;
          host = "0.0.0.0";
        };

        build = {
          enable = true;
          package = cfBuilder;
          offline = false;
        };

        cache = {
          push_after_build = false;
          push_to = null;
        };

        flakes = {
          flake_polling_interval = "1m";
          watched = [{
            name = "test-flake";
            repo_url = "http://gitserver/crystal-forge";
            branch = "main";
            auto_poll = true;
            initial_commit_depth = 5;
          }];
        };

        environments = [{
          name = "test";
          description = "Test environment for mega integration";
          is_active = true;
          risk_profile = "LOW";
          compliance_level = "NONE";
        }];

        systems = [{
          hostname = "mega-test-system";
          public_key =
            lib.strings.trim (builtins.readFile "${pubPath}/agent.pub");
          environment = "test";
          flake_name = "test-flake";
        }];
      };

      systemd.services."crystal-forge-postgres-jobs".enable = lib.mkForce false;
      systemd.timers."crystal-forge-postgres-jobs".enable = lib.mkForce false;

      # Start with local auth
      systemd.services.crystal-forge-server.environment.AUTH_MODE = "local";
    };
  };

  globalTimeout = 2400; # 40 minutes for comprehensive testing

  extraPythonPackages = p: [
    p.pytest
    p.pytest-xdist
    p.pytest-metadata
    p.pytest-html
    p.psycopg2
    p.requests
    pkgs.crystal-forge.cf-test-suite
  ];

  testScript = ''
    import json
    import base64
    import datetime
    import os
    import pytest
    import time

    os.environ["NIXOS_TEST_DRIVER"] = "1"

    producer_started_at = datetime.datetime.now(datetime.timezone.utc).isoformat()
    producer_started = time.monotonic()
    phase_timings = {}

    def start_phase():
        return {
            "startedAt": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "monotonic": time.monotonic(),
        }

    def finish_phase(name, started, status="completed", **details):
        ended = time.monotonic()
        phase_timings[name] = {
            "status": status,
            "startedAt": started["startedAt"],
            "endedAt": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "durationSeconds": round(ended - started["monotonic"], 3),
            **details,
        }

    def skip_phase(name):
        now = datetime.datetime.now(datetime.timezone.utc).isoformat()
        phase_timings[name] = {
            "status": "skipped",
            "startedAt": now,
            "endedAt": now,
            "durationSeconds": 0,
        }

    # Cache + builder mega phases are opt-in. They can only run interactively
    # (the env var cannot cross the Nix build sandbox), so in CI the attic and
    # s3 cache VMs would boot and be health-waited without ever being used.
    # Only start the VMs that this run will actually exercise.
    run_mega_phases = os.environ.get("CF_WEB_UI_RUN_MEGA_PHASES", "0") == "1"

    setup_timing = start_phase()
    machine.start()
    gitserver.start()
    if run_mega_phases:
        atticCache.start()
        s3Cache.start()

    # === Infrastructure Warmup ===
    print("=== Infrastructure Warmup ===")
    machine.wait_for_unit("postgresql.service")
    machine.wait_for_unit("crystal-forge-server.service")
    machine.wait_for_unit("crystal-forge-builder.service")
    machine.wait_for_open_port(${toString CF_TEST_SERVER_PORT})
    machine.wait_for_open_port(5432)

    # Deterministic normalized compliance fixtures used by the real mapping
    # round-trip browser step. These are test-only rows, created after the
    # server has applied migrations and before Playwright starts.
    mapping_fixture_sql = """
      INSERT INTO compliance_frameworks (name, canonical_source_key)
      VALUES ('Test Mapping Framework', 'web-ui-mapping-roundtrip')
      ON CONFLICT (canonical_source_key) DO NOTHING;
      INSERT INTO compliance_framework_versions (framework_id, version, canonical_release_key, semantic_digest)
      SELECT id, '1', 'web-ui-mapping-roundtrip-v1', 'web-ui-mapping-roundtrip-digest'
      FROM compliance_frameworks WHERE canonical_source_key = 'web-ui-mapping-roundtrip'
      ON CONFLICT (framework_id, canonical_release_key) DO NOTHING;
      -- Second release: the bundle baseline step proves that switching
      -- framework releases clears release-specific requirement IDs, so the
      -- framework needs two releases with distinct requirement identifiers.
      INSERT INTO compliance_framework_versions (framework_id, version, canonical_release_key, semantic_digest)
      SELECT id, '2', 'web-ui-mapping-roundtrip-v2', 'web-ui-mapping-roundtrip-digest-v2'
      FROM compliance_frameworks WHERE canonical_source_key = 'web-ui-mapping-roundtrip'
      ON CONFLICT (framework_id, canonical_release_key) DO NOTHING;
      INSERT INTO compliance_requirements (framework_id, canonical_requirement_key)
      SELECT id, 'MAP-1' FROM compliance_frameworks WHERE canonical_source_key = 'web-ui-mapping-roundtrip'
      ON CONFLICT (framework_id, canonical_requirement_key) DO NOTHING;
      INSERT INTO compliance_requirements (framework_id, canonical_requirement_key)
      SELECT id, 'MAP-2' FROM compliance_frameworks WHERE canonical_source_key = 'web-ui-mapping-roundtrip'
      ON CONFLICT (framework_id, canonical_requirement_key) DO NOTHING;
      INSERT INTO compliance_requirement_versions (requirement_id, framework_version_id, external_id, title, kind, semantic_digest)
      SELECT r.id, v.id, 'MAP-1', 'Mapping round-trip requirement one', 'control', 'web-ui-map-1'
      FROM compliance_requirements r JOIN compliance_frameworks f ON f.id = r.framework_id
      JOIN compliance_framework_versions v ON v.framework_id = f.id
      WHERE f.canonical_source_key = 'web-ui-mapping-roundtrip' AND r.canonical_requirement_key = 'MAP-1'
        AND v.canonical_release_key = 'web-ui-mapping-roundtrip-v1'
      ON CONFLICT (requirement_id, framework_version_id) DO NOTHING;
      INSERT INTO compliance_requirement_versions (requirement_id, framework_version_id, external_id, title, kind, semantic_digest)
      SELECT r.id, v.id, 'MAP-1-V2', 'Mapping round-trip requirement one (release 2)', 'control', 'web-ui-map-1-v2'
      FROM compliance_requirements r JOIN compliance_frameworks f ON f.id = r.framework_id
      JOIN compliance_framework_versions v ON v.framework_id = f.id
      WHERE f.canonical_source_key = 'web-ui-mapping-roundtrip' AND r.canonical_requirement_key = 'MAP-1'
        AND v.canonical_release_key = 'web-ui-mapping-roundtrip-v2'
      ON CONFLICT (requirement_id, framework_version_id) DO NOTHING;
      INSERT INTO compliance_requirement_versions (requirement_id, framework_version_id, external_id, title, kind, semantic_digest)
      SELECT r.id, v.id, 'MAP-2', 'Mapping round-trip requirement two', 'control', 'web-ui-map-2'
      FROM compliance_requirements r JOIN compliance_frameworks f ON f.id = r.framework_id
      JOIN compliance_framework_versions v ON v.framework_id = f.id
      WHERE f.canonical_source_key = 'web-ui-mapping-roundtrip' AND r.canonical_requirement_key = 'MAP-2'
        AND v.canonical_release_key = 'web-ui-mapping-roundtrip-v1'
      ON CONFLICT (requirement_id, framework_version_id) DO NOTHING;
    """
    encoded_mapping_fixture_sql = base64.b64encode(
      mapping_fixture_sql.encode("utf-8")
    ).decode("ascii")
    machine.succeed(
      "printf %s " + encoded_mapping_fixture_sql
      + " | base64 -d | sudo -u postgres psql -d crystal_forge -v ON_ERROR_STOP=1"
    )

    from cf_test.vm_helpers import wait_for_git_server_ready
    wait_for_git_server_ready(gitserver, timeout=120)

    if run_mega_phases:
        atticCache.wait_for_unit("atticd.service")
        atticCache.wait_for_open_port(8080)

        try:
            s3Cache.wait_for_unit("garage.service")
        except Exception:
            print(s3Cache.succeed("systemctl status garage.service --no-pager -l || true"))
            print(s3Cache.succeed("journalctl -u garage.service --no-pager -n 200 || true"))
            print(s3Cache.succeed("cat /etc/garage.toml || true"))
            print(s3Cache.succeed("cat /etc/garage/garage.toml || true"))
            raise

        s3Cache.wait_for_open_port(3900)

    # Set up test environment variables
    main_head = "${
      lib.strings.trim
      (builtins.readFile (lib.crystal-forge.testFlake + "/MAIN_HEAD"))
    }"

    os.environ.update({
      "CF_TEST_GIT_SERVER_URL": "http://gitserver/crystal-forge",
      "CF_TEST_REAL_REPO_URL": "http://gitserver/crystal-forge",
      "CF_TEST_REAL_COMMIT_HASH": main_head,
      "CF_TEST_DB_HOST": "127.0.0.1",
      "CF_TEST_DB_PORT": "5433",
      "CF_TEST_DB_USER": "postgres",
      "CF_TEST_DB_PASSWORD": "",
      "CF_TEST_SERVER_HOST": "127.0.0.1",
      "CF_TEST_SERVER_PORT": "${toString CF_TEST_SERVER_PORT}",
      "CF_TEST_DRV": "${derivation-paths}",
      "CF_TEST_FLAKE_NAME": "test-flake",
    })

    machine.forward_port(5433, 5432)
    machine.forward_port(${toString CF_TEST_SERVER_PORT}, ${
      toString CF_TEST_SERVER_PORT
    })

    import cf_test
    cf_test._driver_machines = {
      "machine": machine,
      "cfServer": machine,
      "gitserver": gitserver,
      "atticCache": atticCache,
      "s3Cache": s3Cache,
    }
    finish_phase("vmFixtureSetup", setup_timing)

    # Optional mega phases (cache + builder) are flaky in constrained CI VMs.
    # Keep them opt-in so the web-ui check remains focused on Playwright UI validation.
    if run_mega_phases:
      # === Phase 1: Attic Cache Tests ===
      print("=== Phase 1: Attic Cache Tests ===")
      exit_code = pytest.main([
        "-vvvv", "--tb=short", "-x", "-s",
        "-m", "attic_cache", "--pyargs", "cf_test",
      ])
      if exit_code != 0:
        raise SystemExit(exit_code)

      # === Phase 2: S3 Cache Tests ===
      print("=== Phase 2: S3 Cache Tests ===")
      exit_code = pytest.main([
        "-vvvv", "--tb=short", "-x", "-s",
        "-m", "s3cache", "--pyargs", "cf_test",
      ])
      if exit_code != 0:
        raise SystemExit(exit_code)

      # === Phase 3: Builder Tests ===
      print("=== Phase 3: Builder Tests ===")
      exit_code = pytest.main([
        "-vvvv", "--tb=short", "-x", "-s",
        "-m", "builder", "--pyargs", "cf_test",
      ])
      if exit_code != 0:
        raise SystemExit(exit_code)
    else:
      print("=== Skipping mega non-UI phases (set CF_WEB_UI_RUN_MEGA_PHASES=1 to enable) ===")

    browser_ok = ${if runBrowserSemanticValidation then "True" else "None"}
    visual_ok = ${if runBrowserSemanticValidation then "True" else "None"}
    oscal_ok = True
    sarif_ok = True
    design_parity = {
        "status": "skipped",
        "ok": None,
        "commandStatuses": {},
        "missingOutputs": [],
    }

    # === Phase 4a: Web UI Build Verification ===
    if ${if runAssetVerification then "True" else "False"}:
      print("=== Phase 4a: Web UI Build Verification ===")

    # Every partition verifies server readiness. Only the compatibility check
    # verifies the complete embedded asset chain as its production smoke role.
    machine.succeed("curl -sf http://127.0.0.1:${
      toString CF_TEST_SERVER_PORT
    }/status | jq .")
    print("Server is up and responding")

    if ${if runAssetVerification then "True" else "False"}:
      # Verify index.html, its JS loader, and the packaged WASM magic header.
      print(machine.succeed("${verifyWebUiAssets} http://127.0.0.1:${
        toString CF_TEST_SERVER_PORT
      } ${pkgs.crystal-forge.web-ui}/public"))

    # === Phase 4: Web UI Tests (Playwright) ===
    print("=== Phase 4: Web UI Tests (Playwright) ===")

    # Create output directories
    machine.succeed("mkdir -p /tmp/screenshots")
    machine.succeed("mkdir -p /tmp/web-ui-tests")

    # Copy test files and coverage manifest into the VM
    machine.succeed("cp -r ${testDir}/* /tmp/web-ui-tests/")
    machine.succeed("cp ${coverageManifest} /tmp/web-ui-tests/coverage-manifest.json")
    machine.succeed("cp ${checkGroups} /tmp/web-ui-tests/check-groups.json")
    machine.succeed("mkdir -p /tmp/web-ui-baselines && cp -r ${baselinesDir}/. /tmp/web-ui-baselines/")
    machine.succeed("cp ${./default.nix} /tmp/web-ui-tests/default.nix")
    machine.succeed(
        "env CF_WEB_UI_SOURCE_DIR=/tmp/web-ui-tests CF_UI_STATIC_CONTRACTS=1 "
        "${pkgs.nodejs}/bin/node /tmp/web-ui-tests/integration-test.js"
    )

    # Design-parity harness inputs: scripts + manifest (read by integration-test.js
    # at /tmp/web-ui-tests/design-parity/manifest.json) and the offline design
    # example bundle.
    ${lib.optionalString runDesignParity ''
machine.succeed("mkdir -p /tmp/web-ui-tests/design-parity")
machine.succeed("cp -r ${designParityDir}/. /tmp/web-ui-tests/design-parity/")
machine.succeed("mkdir -p /tmp/design-example && cp -r ${designExampleOffline}/. /tmp/design-example/")
machine.succeed("${pkgs.nodejs}/bin/node /tmp/web-ui-tests/design-parity/generate-design-targets-test.js")
''}

    ${lib.optionalString runBrowserSemanticValidation ''
    browser_timing = start_phase()
    test_profile = "${testProfile}"
    test_steps = ${if testSteps == null then "None" else "\"${testSteps}\""}
    test_steps_env = f" CF_UI_TEST_STEPS={test_steps}" if test_steps else ""
    # The browser process gets 15 minutes of the 20-minute gate budget. This
    # leaves five minutes for VM setup and evidence publication while remaining
    # well above healthy shard browser runtimes. The driver waits one additional
    # minute so the wrapper can terminate Chromium and publish integration.exit.
    process_timeout = ${toString effectivePlaywrightProcessTimeout}
    result_timeout = ${toString playwrightResultTimeout}

    # Deployment-policy fixture state. Browser steps that read the policy
    # catalog depend on it, so record the shape of the seeded data to tell
    # fixture drift apart from a server-side catalog failure.
    print("=== Deployment policy fixture state ===")
    print(
        machine.succeed(
            "sudo -u postgres psql -d crystal_forge -A -t -c "
            "\"SELECT (SELECT COUNT(*) FROM deployment_policies) AS policies, "
            "(SELECT COUNT(*) FROM deployment_policy_versions) AS versions, "
            "(SELECT COUNT(*) FROM deployment_policy_versions "
            "WHERE compliance_metadata ? 'evidence_specs') AS with_evidence\" || true"
        )
    )

    # Run the integration test script
    machine.succeed("rm -f /tmp/web-ui-tests/integration.exit /tmp/web-ui-tests/integration.exit.tmp /tmp/screenshots/results.json /tmp/screenshots/verdict.json /tmp/screenshots/fatal.json /tmp/screenshots/current-step.json")
    machine.succeed(
        f"nohup sh -c '${pkgs.coreutils}/bin/timeout --signal=TERM --kill-after=30s "
        f"{process_timeout}s env CF_UI_BASELINES_DIR=/tmp/web-ui-baselines "
        f"CF_UI_TEST_PROFILE={test_profile} CF_UI_SKIP_DESIGN_PARITY=${if runDesignParity then "0" else "1"}{test_steps_env} ${pkgs.nodejs}/bin/node /tmp/web-ui-tests/integration-test.js http://127.0.0.1:${
          toString CF_TEST_SERVER_PORT
        } /tmp/screenshots; status=$?; printf \"%s\\n\" \"$status\" > /tmp/web-ui-tests/integration.exit.tmp; mv /tmp/web-ui-tests/integration.exit.tmp /tmp/web-ui-tests/integration.exit' > /tmp/web-ui-tests/integration.log 2>&1 </dev/null &"
    )
    def export_failure_artifacts():
        try:
            machine.succeed(
                "journalctl -u crystal-forge-server.service --no-pager -n 300 "
                "> /tmp/web-ui-tests/server-journal.log 2>&1 || true; "
                "rm -rf /tmp/web-ui-failure-artifacts && "
                "mkdir -p /tmp/web-ui-failure-artifacts && "
                "cp -a /tmp/screenshots/. /tmp/web-ui-failure-artifacts/ && "
                "cp /tmp/web-ui-tests/integration.log /tmp/web-ui-failure-artifacts/integration.log && "
                "cp /tmp/web-ui-tests/server-journal.log /tmp/web-ui-failure-artifacts/server-journal.log && "
                "if test -f /tmp/web-ui-tests/integration.exit; then "
                "cp /tmp/web-ui-tests/integration.exit /tmp/web-ui-failure-artifacts/integration.exit; fi"
            )
            machine.copy_from_vm("/tmp/web-ui-failure-artifacts", "browser-failure-artifacts")
        except Exception as e:
            print(f"warning: could not export browser failure artifacts: {e}")

    try:
        machine.wait_for_file("/tmp/web-ui-tests/integration.exit", timeout=result_timeout)
    except Exception:
        print(machine.succeed("cat /tmp/web-ui-tests/integration.log || true"))
        print(machine.succeed("journalctl -u crystal-forge-server.service --no-pager -n 300 || true"))
        export_failure_artifacts()
        raise
    output = machine.succeed("cat /tmp/web-ui-tests/integration.log")
    print(output)

    exit_code = machine.succeed("cat /tmp/web-ui-tests/integration.exit").strip()

    if exit_code in ["124", "137"]:
        print("=== Web UI browser shard timeout ===")
        print("Current step:")
        print(machine.succeed("cat /tmp/screenshots/current-step.json 2>/dev/null || printf '%s\\n' '{\"error\":\"current step unavailable\"}'"))
        print("=== Crystal Forge server journal after browser shard timeout ===")
        print(
            machine.succeed(
                "journalctl -u crystal-forge-server.service --no-pager -n 300 || true"
            )
        )
        export_failure_artifacts()
        raise Exception(
            f"Web UI browser shard exceeded the {process_timeout}-second process timeout "
            f"(exit code {exit_code}); no logical verdict was produced"
        )

    # Coverage-gate failures (manifest drift) abort before any results exist.
    if machine.execute("test -f /tmp/screenshots/fatal.json")[0] == 0:
        export_failure_artifacts()
        fatal_json = machine.succeed("cat /tmp/screenshots/fatal.json")
        raise Exception(f"Web UI check aborted: {json.loads(fatal_json)['error']}")

    if machine.execute("test -f /tmp/screenshots/results.json")[0] != 0:
        export_failure_artifacts()
        print("=== Crystal Forge server journal after integration failure ===")
        print(
            machine.succeed(
                "journalctl -u crystal-forge-server.service --no-pager -n 300 || true"
            )
        )
        raise Exception(
            "integration process exited before producing results.json "
            f"(exit code {exit_code})"
        )

    # Read results
    results_json = machine.succeed("cat /tmp/screenshots/results.json")
    results = json.loads(results_json)
    verdict_json = machine.succeed("cat /tmp/screenshots/verdict.json")
    verdict = json.loads(verdict_json)

    if any(not result.get("ok") for result in results):
        print("=== Crystal Forge server journal after failed browser step ===")
        print(
            machine.succeed(
                "journalctl -u crystal-forge-server.service --no-pager -n 300 || true"
            )
        )

    # Copy final, intermediate, and partial diagnostic screenshots out.
    for r in results:
        for visual in r.get("visuals", []):
            if machine.execute(f"test -f /tmp/screenshots/{visual['name']}.png")[0] == 0:
                machine.copy_from_vm(f"/tmp/screenshots/{visual['name']}.png", "screenshots")
        if not r.get("ok") and machine.execute(f"test -f /tmp/screenshots/{r['name']}.png")[0] == 0:
            machine.copy_from_vm(f"/tmp/screenshots/{r['name']}.png", "screenshots")

    for report_file in ["results.json", "verdict.json", "visual-report.json", "visual-summary.md"]:
        try:
            machine.copy_from_vm(f"/tmp/screenshots/{report_file}", "screenshots")
        except Exception as e:
            print(f"warning: could not copy {report_file}: {e}")
    if machine.execute("test -d /tmp/screenshots/diffs")[0] == 0:
        machine.copy_from_vm("/tmp/screenshots/diffs", "screenshots")

    # === Visual Design-Parity Comparison (NON-BLOCKING) ===
    # Render the tracked design example (offline, shared fixture) for the primary
    # views/themes, then compare against the real Dioxus captures produced by the
    # integration test above. This is the primary visual comparison — the design
    # example IS the baseline. Results are reported as a directional drift gauge
    # and a summary matrix, but the check never fails on visual mismatch alone.
    ${lib.optionalString runDesignParity ''
print("=== Design-Parity Visual Comparison (design vs Dioxus, non-blocking) ===")
design_timing = start_phase()
try:
    machine.succeed("mkdir -p /tmp/design-targets /tmp/design-parity")
    generate_status, _ = machine.execute(
        "${pkgs.nodejs}/bin/node /tmp/web-ui-tests/design-parity/generate-design-targets.js "
        "/tmp/design-example /tmp/web-ui-tests/design-parity/manifest.json /tmp/design-targets "
        "> /tmp/web-ui-tests/design-targets.log 2>&1"
    )
    print(machine.succeed("cat /tmp/web-ui-tests/design-targets.log"))
    compare_status, _ = machine.execute(
        "${pkgs.nodejs}/bin/node /tmp/web-ui-tests/design-parity/compare-design-parity.js "
        "/tmp/web-ui-tests/design-parity/manifest.json /tmp/design-targets "
        "/tmp/screenshots/design-parity /tmp/design-parity "
        "> /tmp/web-ui-tests/design-parity.log 2>&1"
    )
    print(machine.succeed("cat /tmp/web-ui-tests/design-parity.log"))

    expected_design_outputs = {
        "design-drift-report.json": "/tmp/design-parity/design-drift-report.json",
        "design-drift-summary.md": "/tmp/design-parity/design-drift-summary.md",
        "design-parity-matrix.png": "/tmp/design-parity/design-parity-matrix.png",
        "montages": "/tmp/design-parity/montages",
        "design-targets": "/tmp/design-targets",
        "design-parity": "/tmp/screenshots/design-parity",
    }
    missing_design_outputs = [
        output for output, output_path in expected_design_outputs.items()
        if machine.execute(f"test -e {output_path}")[0] != 0
    ]
    design_parity = {
        "status": (
            "failed" if generate_status != 0 or compare_status != 0
            else "missing-output" if missing_design_outputs
            else "passed"
        ),
        "ok": generate_status == 0 and compare_status == 0 and not missing_design_outputs,
        "commandStatuses": {
            "generateTargets": generate_status,
            "compare": compare_status,
        },
        "missingOutputs": missing_design_outputs,
    }

    # Copy design-parity artifacts out (report, summary, montages, matrix grid, raw sides).
    for report_file in ["design-drift-report.json", "design-drift-summary.md"]:
        if machine.execute(f"test -f /tmp/design-parity/{report_file}")[0] == 0:
            machine.copy_from_vm(f"/tmp/design-parity/{report_file}", "screenshots")
    if machine.execute("test -d /tmp/design-parity/montages")[0] == 0:
        machine.copy_from_vm("/tmp/design-parity/montages", "screenshots")
    for grid_file in ["design-parity-matrix.png"]:
        if machine.execute(f"test -f /tmp/design-parity/{grid_file}")[0] == 0:
            machine.copy_from_vm(f"/tmp/design-parity/{grid_file}", "screenshots")
    if machine.execute("test -d /tmp/design-targets")[0] == 0:
        machine.copy_from_vm("/tmp/design-targets", "screenshots")
    if machine.execute("test -d /tmp/screenshots/design-parity")[0] == 0:
        machine.copy_from_vm("/tmp/screenshots/design-parity", "screenshots")
except Exception as e:
    design_parity = {
        "status": "failed",
        "ok": False,
        "commandStatuses": design_parity.get("commandStatuses", {}),
        "missingOutputs": design_parity.get("missingOutputs", []),
        "error": str(e),
    }
    print(f"warning: design-parity harness error (advisory): {e}")
finish_phase(
    "designParity",
    design_timing,
    status="completed" if design_parity["ok"] else "failed",
)
''}
    ${lib.optionalString (!runDesignParity) ''
skip_phase("designParity")
''}

    ok_count = sum(1 for r in results if r.get("ok"))

    print("\n=== Summary ===")
    print(f"  Screenshots: {ok_count}/{len(results)} captured")

    for r in results:
        status = "OK" if r.get("ok") else "FAIL"
        error = r.get("error", "")
        if error:
            print(f"  [{status}] {r['name']} - {error}")
        else:
            print(f"  [{status}] {r['name']}")

    for failure in verdict.get("failedRequiredSteps", verdict.get("failedSteps", [])):
        print(f"  REQUIRED STEP FAILED: {failure['name']} - {failure['reason']}")
    for failure in verdict.get("failedAdvisorySteps", []):
        print(f"  ADVISORY STEP FAILED: {failure['name']} - {failure['reason']}")
    if verdict.get("processError"):
        print(f"  PROCESS FAILURE: {verdict['processError']}")

    if exit_code != "0" or not verdict.get("ok"):
        browser_ok = False
        failures = [
            f"{failure['name']}: {failure['reason']}"
            for failure in verdict.get("failedRequiredSteps", verdict.get("failedSteps", []))
        ]
        if verdict.get("processError"):
            failures.append(verdict["processError"])
        print(
            "Selected web UI checks failed "
            f"(exit code {exit_code}): {failures or ['no failure detail was recorded']}"
        )

    # Strict visual drift is a logical failure. Keep the evidence derivation
    # successful so the outer gate can reject it after artifacts are retained.
    visual_report_json = machine.succeed("cat /tmp/screenshots/visual-report.json")
    visual_report = json.loads(visual_report_json)
    visual_counts = visual_report.get("counts")
    required_visual_counts = {"match", "diff", "new", "skipped", "error"}
    if not isinstance(visual_counts, dict) or not required_visual_counts.issubset(visual_counts):
        raise Exception("visual-report.json has an invalid counts schema")
    if any(
        not isinstance(visual_counts[key], int) or visual_counts[key] < 0
        for key in required_visual_counts
    ):
        raise Exception("visual-report.json contains invalid visual counts")
    strict_visual_failures = visual_report.get("failures")
    if not isinstance(strict_visual_failures, list):
        raise Exception("visual-report.json has an invalid failures schema")
    visual_ok = not strict_visual_failures or ${if updateVisualBaselines then "True" else "False"}
    for failure in strict_visual_failures:
        print(f"  STRICT VISUAL FAIL: {failure['name']} ({failure['status']})")
    if strict_visual_failures and ${if updateVisualBaselines then "True" else "False"}:
        print("  Baseline update mode: strict visual failures are exported for review")
    print(f"  Themed screenshot captures: {visual_report.get('themedCaptures', 0)}")
    finish_phase("browserSemanticExecution", browser_timing)
''}
    ${lib.optionalString (!runBrowserSemanticValidation) ''
    skip_phase("browserSemanticExecution")
''}

    # Export processes always write step-level results for logical failures.
    ${lib.optionalString runExportValidation ''
# A missing result remains an infrastructure failure of this VM evidence.
export_timing = start_phase()
print("=== Phase 5: OSCAL Export Validation ===")
oscal_status, _ = machine.execute(
    f"${pkgs.nodejs}/bin/node /tmp/web-ui-tests/oscal-export-test.js"
    f" http://127.0.0.1:${toString CF_TEST_SERVER_PORT}"
    f" /tmp/screenshots ${pkgs.crystal-forge.oscal-1-1-2-schemas}"
    f" > /tmp/web-ui-tests/oscal-export.log 2>&1"
)
print(machine.succeed("cat /tmp/web-ui-tests/oscal-export.log"))
oscal_results = json.loads(
    machine.succeed("cat /tmp/screenshots/oscal-export-results.json")
)
oscal_ok = oscal_status == 0 and all(result.get("ok") for result in oscal_results)
for result in oscal_results:
    if not result.get("ok"):
        print(f"  FAIL: {result['name']} - {result.get('error', 'unknown error')}")

print("=== Phase 6: SARIF Export Validation ===")
sarif_schema_file = "${pkgs.crystal-forge.sarif-2-1-0-schema}/sarif-schema-2.1.0.json"
sarif_status, _ = machine.execute(
    f"${pkgs.nodejs}/bin/node /tmp/web-ui-tests/sarif-export-test.js"
    f" http://127.0.0.1:${toString CF_TEST_SERVER_PORT}"
    f" /tmp/screenshots {sarif_schema_file}"
    f" > /tmp/web-ui-tests/sarif-export.log 2>&1"
)
print(machine.succeed("cat /tmp/web-ui-tests/sarif-export.log"))
sarif_results = json.loads(
    machine.succeed("cat /tmp/screenshots/sarif-export-results.json")
)
sarif_ok = sarif_status == 0 and all(result.get("ok") for result in sarif_results)
for result in sarif_results:
    if not result.get("ok"):
        print(f"  FAIL: {result['name']} - {result.get('error', 'unknown error')}")

for report_file in [
    "oscal-export-results.json",
    "oscal-export-final.png",
    "sarif-export-results.json",
    "sarif-export-final.png",
]:
    if machine.execute(f"test -f /tmp/screenshots/{report_file}")[0] == 0:
        machine.copy_from_vm(f"/tmp/screenshots/{report_file}", "screenshots")
finish_phase("exports", export_timing)
''}
    ${lib.optionalString (!runExportValidation) ''
skip_phase("exports")
''}

    # Logical failures are data in the evidence derivation. The outer gate
    # reads this verdict and fails without invoking the VM a second time.
    finalization_timing = start_phase()
    check_verdict = {
        "schemaVersion": 2,
        "check": "${checkName}",
        "blocking": ${if blocking then "True" else "False"},
        "ok": (${if gateBrowserValidation then "browser_ok and visual_ok and " else ""}${if runDesignParity then "design_parity[\"ok\"] and " else ""}oscal_ok and sarif_ok),
        "components": {
            "browser": browser_ok,
            "visualEvidence": visual_ok,
            "designParity": design_parity["ok"],
            "oscalExport": oscal_ok,
            "sarifExport": sarif_ok,
        },
        "designParity": design_parity,
        "strictVisualFailures": strict_visual_failures if ${if runBrowserSemanticValidation then "True" else "False"} else [],
        "failedRequiredSteps": verdict.get("failedRequiredSteps", verdict.get("failedSteps", [])) if ${if runBrowserSemanticValidation then "True" else "False"} else [],
        "failedAdvisorySteps": verdict.get("failedAdvisorySteps", []) if ${if runBrowserSemanticValidation then "True" else "False"} else [],
    }
    encoded_check_verdict = base64.b64encode(
        json.dumps(check_verdict, indent=2).encode("utf-8")
    ).decode("ascii")
    machine.succeed(
        "printf %s " + encoded_check_verdict
        + " | base64 -d > /tmp/screenshots/check-verdict.json"
    )
    machine.copy_from_vm("/tmp/screenshots/check-verdict.json", "screenshots")
    finish_phase("evidenceFinalization", finalization_timing)
    producer_ended_at = datetime.datetime.now(datetime.timezone.utc).isoformat()
    timing_result = {
        "schemaVersion": 1,
        "check": "${checkName}",
        "startedAt": producer_started_at,
        "endedAt": producer_ended_at,
        "durationSeconds": round(time.monotonic() - producer_started, 3),
        "phases": phase_timings,
    }
    encoded_timings = base64.b64encode(
        json.dumps(timing_result, indent=2).encode("utf-8")
    ).decode("ascii")
    machine.succeed(
        "printf %s " + encoded_timings
        + " | base64 -d > /tmp/screenshots/phase-timings.json"
    )
    machine.copy_from_vm("/tmp/screenshots/phase-timings.json", "screenshots")
  '';
  };
in pkgs.runCommand "crystal-forge-${checkName}-gate"
  {
    nativeBuildInputs = [ pkgs.nodejs ];
    passthru = {
      inherit evidence;
      # INVARIANT: Wrappers select behavior only. They must not replace these
      # shared derivations, or parallel checks lose cross-job cache reuse.
      sharedInputs = {
        server = cfServer;
        agent = cfAgent;
        builder = cfBuilder;
        webUi = pkgs.crystal-forge.web-ui;
        chromium = pkgs.chromium;
        browserBundle = pkgs.playwright-driver;
        inherit verifyWebUiAssets;
      };
    };
  }
  ''
    mkdir -p $out
    ln -s ${evidence}/screenshots $out/screenshots
    ${lib.optionalString blocking ''
      ${pkgs.nodejs}/bin/node ${gateVerdictChecker} ${evidence}/screenshots/check-verdict.json
    ''}
  ''
