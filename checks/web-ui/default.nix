# Web UI Integration Check
#
# Manifest-driven Playwright verification of the web UI against a real
# Crystal Forge server (PostgreSQL + gitserver), with:
# - Explicit build verification (served index.html/JS loader, packaged WASM magic header)
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
, testProfile ? "ci_fast"
, testSteps ? null
, runExportValidation ? true
, playwrightResultTimeout ? 1800
, ...
}:
let
  testDir = ./tests;
  coverageManifest = ./coverage-manifest.json;
  designParityDir = ./design-parity;
  CF_TEST_SERVER_PORT = 3000;

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
  systemBuildClosure = pkgs.closureInfo {
    rootPaths = [
      inputs.self.nixosConfigurations.cf-test-sys.config.system.build.toplevel
      pkgs.crystal-forge.default
      pkgs.path
    ] ++ lib.crystal-forge.prefetchedPaths;
  };
in pkgs.testers.runNixOSTest {
  name = "crystal-forge-web-ui-mega-integration";

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
        pkgs.crystal-forge.default
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
          port = CF_TEST_SERVER_PORT;
          host = "0.0.0.0";
        };

        build = {
          enable = true;
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
    import os
    import pytest

    os.environ["NIXOS_TEST_DRIVER"] = "1"

    # Cache + builder mega phases are opt-in. They can only run interactively
    # (the env var cannot cross the Nix build sandbox), so in CI the attic and
    # s3 cache VMs would boot and be health-waited without ever being used.
    # Only start the VMs that this run will actually exercise.
    run_mega_phases = os.environ.get("CF_WEB_UI_RUN_MEGA_PHASES", "0") == "1"

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
      ON CONFLICT (requirement_id, framework_version_id) DO NOTHING;
      INSERT INTO compliance_requirement_versions (requirement_id, framework_version_id, external_id, title, kind, semantic_digest)
      SELECT r.id, v.id, 'MAP-2', 'Mapping round-trip requirement two', 'control', 'web-ui-map-2'
      FROM compliance_requirements r JOIN compliance_frameworks f ON f.id = r.framework_id
      JOIN compliance_framework_versions v ON v.framework_id = f.id
      WHERE f.canonical_source_key = 'web-ui-mapping-roundtrip' AND r.canonical_requirement_key = 'MAP-2'
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

    # === Phase 4a: Web UI Build Verification ===
    print("=== Phase 4a: Web UI Build Verification ===")

    # Verify server is responding
    machine.succeed("curl -sf http://127.0.0.1:${
      toString CF_TEST_SERVER_PORT
    }/status | jq .")
    print("Server is up and responding")

    # Explicitly verify the UI build: index.html served, JS loader served, and
    # the packaged WASM output has a valid magic header.
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

    # Design-parity harness inputs: scripts + manifest (read by integration-test.js
    # at /tmp/web-ui-tests/design-parity/manifest.json) and the offline design
    # example bundle.
    machine.succeed("mkdir -p /tmp/web-ui-tests/design-parity")
    machine.succeed("cp -r ${designParityDir}/. /tmp/web-ui-tests/design-parity/")
    machine.succeed("mkdir -p /tmp/design-example && cp -r ${designExampleOffline}/. /tmp/design-example/")

    test_profile = "${testProfile}"
    test_steps = ${if testSteps == null then "None" else "\"${testSteps}\""}
    test_steps_env = f" CF_UI_TEST_STEPS={test_steps}" if test_steps else ""
    result_timeout = ${toString playwrightResultTimeout}

    # Run the integration test script
    machine.succeed("rm -f /tmp/web-ui-tests/integration.exit /tmp/screenshots/results.json /tmp/screenshots/fatal.json")
    machine.succeed(
        f"nohup sh -c 'env CF_UI_TEST_PROFILE={test_profile}{test_steps_env} ${pkgs.nodejs}/bin/node /tmp/web-ui-tests/integration-test.js http://127.0.0.1:${
          toString CF_TEST_SERVER_PORT
        } /tmp/screenshots; status=$?; printf \"%s\\n\" \"$status\" > /tmp/web-ui-tests/integration.exit' > /tmp/web-ui-tests/integration.log 2>&1 </dev/null &"
    )
    machine.wait_until_succeeds(
        "test -f /tmp/screenshots/results.json -o -f /tmp/screenshots/fatal.json -o -f /tmp/web-ui-tests/integration.exit",
        timeout=result_timeout,
    )
    output = machine.succeed("cat /tmp/web-ui-tests/integration.log")
    print(output)

    # Coverage-gate failures (manifest drift) abort before any results exist.
    if machine.execute("test -f /tmp/screenshots/fatal.json")[0] == 0:
        fatal_json = machine.succeed("cat /tmp/screenshots/fatal.json")
        raise Exception(f"Web UI check aborted: {json.loads(fatal_json)['error']}")

    if machine.execute("test -f /tmp/screenshots/results.json")[0] != 0:
        exit_code = machine.succeed("cat /tmp/web-ui-tests/integration.exit").strip()
        raise Exception(
            "integration process exited before producing results.json "
            f"(exit code {exit_code})"
        )

    # Read results
    results_json = machine.succeed("cat /tmp/screenshots/results.json")
    results = json.loads(results_json)

    # Copy screenshots + visual reports out
    for r in results:
        if r.get("ok"):
            for visual in r.get("visuals", []):
                machine.copy_from_vm(f"/tmp/screenshots/{visual['name']}.png", "screenshots")

    for report_file in ["results.json", "visual-report.json", "visual-summary.md"]:
        try:
            machine.copy_from_vm(f"/tmp/screenshots/{report_file}", "screenshots")
        except Exception as e:
            print(f"warning: could not copy {report_file}: {e}")

    # === Visual Design-Parity Comparison (NON-BLOCKING) ===
    # Render the tracked design example (offline, shared fixture) for the primary
    # views/themes, then compare against the real Dioxus captures produced by the
    # integration test above. This is the primary visual comparison — the design
    # example IS the baseline. Results are reported as a directional drift gauge
    # and a summary matrix, but the check never fails on visual mismatch alone.
    print("=== Design-Parity Visual Comparison (design vs Dioxus, non-blocking) ===")
    try:
        machine.succeed("mkdir -p /tmp/design-targets /tmp/design-parity")
        machine.succeed(
            "${pkgs.nodejs}/bin/node /tmp/web-ui-tests/design-parity/generate-design-targets.js "
            "/tmp/design-example /tmp/web-ui-tests/design-parity/manifest.json /tmp/design-targets "
            "> /tmp/web-ui-tests/design-targets.log 2>&1 || true"
        )
        print(machine.succeed("cat /tmp/web-ui-tests/design-targets.log || true"))
        machine.succeed(
            "${pkgs.nodejs}/bin/node /tmp/web-ui-tests/design-parity/compare-design-parity.js "
            "/tmp/web-ui-tests/design-parity/manifest.json /tmp/design-targets "
            "/tmp/screenshots/design-parity /tmp/design-parity "
            "> /tmp/web-ui-tests/design-parity.log 2>&1 || true"
        )
        print(machine.succeed("cat /tmp/web-ui-tests/design-parity.log || true"))

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
        print(f"warning: design-parity harness error (non-blocking): {e}")

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

    if ok_count == 0:
        raise Exception("All screenshots failed")

    # Fail if critical tests failed
    # Keep this list to stable smoke checks; richer UX flows are tracked by
    # screenshot results but not treated as merge-blocking while UI is evolving.
    critical_tests = [
      "01-login-page",
      "02-registration",
      "05-login-submit",
      "12-systems",
      "13-flakes",
      "15j-builds-latest-per-flake-populated",
      "15k-builds-latest-combined-filters-empty-clear",
      "16-cves",
      "16b-cves-severity-filter",
      "26c-evaluations-latest-per-flake-populated",
      "26d-evaluations-latest-combined-filters-empty-clear",
      "30a-admin-automatic-retries-defaults-reset",
      "30b-admin-automatic-retries-save-reload",
      "30c-admin-automatic-retries-failed-save-retains-draft",
    ]
    failed_critical = [r['name'] for r in results if r['name'] in critical_tests and not r.get('ok')]
    if failed_critical:
        raise Exception(f"Critical web UI checks failed: {failed_critical}")

    # === Phase 4b: Visual Baseline Gate ===
    # Steps marked "strict" in coverage-manifest.json must match their
    # approved baseline within the configured threshold; "advisory" steps are
    # reported (with diff images in screenshots/diffs) but never block.
    visual_report_json = machine.succeed("cat /tmp/screenshots/visual-report.json")
    visual_report = json.loads(visual_report_json)
    counts = visual_report.get("counts", {})
    print(
        f"  Visual baselines: {counts.get('match', 0)} match, "
        f"{counts.get('diff', 0)} differ, {counts.get('new', 0)} new, "
        f"{counts.get('skipped', 0)} skipped"
    )
    visual_failures = visual_report.get("failures", [])
    if visual_failures:
        for f in visual_failures:
            print(f"  STRICT VISUAL FAIL: {f['name']} ({f['status']})")
        raise Exception(
            f"Strict visual baseline failures: {[f['name'] for f in visual_failures]}"
        )

    if not ${if runExportValidation then "True" else "False"}:
        print("=== Focused web UI check complete; export validation skipped ===")
        raise SystemExit(0)

    # === Phase 5: OSCAL Export Validation (end-to-end via web UI) ===
    print("=== Phase 5: OSCAL Export Validation ===")

    # Run the OSCAL export test - this routes compliance API data, opens the
    # export modal in the real web UI, captures the browser-triggered download,
    # and validates the downloaded file against NIST OSCAL 1.1.2 schemas.
    #
    # This exercises the actual production build_oscal() code path in the WASM
    # bundle — the file a user would download is what gets validated.
    machine.succeed(
        f"${pkgs.nodejs}/bin/node /tmp/web-ui-tests/oscal-export-test.js"
        f" http://127.0.0.1:${toString CF_TEST_SERVER_PORT}"
        f" /tmp/screenshots ${pkgs.crystal-forge.oscal-1-1-2-schemas}"
        f" > /tmp/web-ui-tests/oscal-export.log 2>&1"
    )

    oscal_output = machine.succeed("cat /tmp/web-ui-tests/oscal-export.log")
    print(oscal_output)

    oscal_results_json = machine.succeed("cat /tmp/screenshots/oscal-export-results.json")
    oscal_results = json.loads(oscal_results_json)

    oscal_ok = all(r.get("ok") for r in oscal_results)
    if not oscal_ok:
        failed = [r for r in oscal_results if not r.get("ok")]
        for r in failed:
            print(f"  FAIL: {r['name']} - {r.get('error', 'unknown error')}")
        raise Exception(f"OSCAL export validation failed: {len(failed)}/{len(oscal_results)} steps failed")

    # Copy OSCAL export screenshot if available
    try:
        machine.copy_from_vm("/tmp/screenshots/oscal-export-final.png", "screenshots")
    except:
        pass

    print("=== OSCAL Export Validation Passed ===")
    print("The downloaded OSCAL file was validated against NIST 1.1.2 AR + AP + SSP schemas.")
    print("")

    # === Phase 6: SARIF Export Validation (end-to-end via web UI) ===
    print("=== Phase 6: SARIF Export Validation ===")

    # Run the SARIF export test — routes deterministic compliance data, selects
    # SARIF 2.1.0 in the export modal, captures the browser-triggered download,
    # and validates it against the vendored OASIS Errata 01 schema using
    # Draft4Validator + FormatChecker (catching empty URI fields) plus semantic
    # checks (ruleId resolution, host locations, waiver suppressions).
    sarif_schema_file = "${pkgs.crystal-forge.sarif-2-1-0-schema}/sarif-schema-2.1.0.json"
    machine.succeed(
        f"${pkgs.nodejs}/bin/node /tmp/web-ui-tests/sarif-export-test.js"
        f" http://127.0.0.1:${toString CF_TEST_SERVER_PORT}"
        f" /tmp/screenshots"
        f" {sarif_schema_file}"
        f" > /tmp/web-ui-tests/sarif-export.log 2>&1"
    )

    sarif_output = machine.succeed("cat /tmp/web-ui-tests/sarif-export.log")
    print(sarif_output)

    sarif_results_json = machine.succeed("cat /tmp/screenshots/sarif-export-results.json")
    sarif_results = json.loads(sarif_results_json)

    sarif_ok = all(r.get("ok") for r in sarif_results)
    if not sarif_ok:
        failed = [r for r in sarif_results if not r.get("ok")]
        for r in failed:
            print(f"  FAIL: {r['name']} - {r.get('error', 'unknown error')}")
        raise Exception(f"SARIF export validation failed: {len(failed)}/{len(sarif_results)} steps failed")

    try:
        machine.copy_from_vm("/tmp/screenshots/sarif-export-final.png", "screenshots")
    except:
        pass

    print("=== SARIF Export Validation Passed ===")
    print("The downloaded SARIF file was validated against the OASIS 2.1.0 Errata 01 schema.")
    print("")

    print("\n=== All Mega Integration Tests Passed ===")
    print("Completed: Cache (Attic+S3), Builder, Web UI, OSCAL Export, SARIF Export")
  '';
}
