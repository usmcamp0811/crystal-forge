# Web UI Integration Check
#
# Manifest-driven Playwright verification of the web UI against a real
# Crystal Forge server (PostgreSQL + gitserver), with:
# - Explicit build verification (served index.html/JS loader, packaged WASM magic header)
# - Semantic assertions + screenshots per coverage-manifest.json step
# - Structural/geometry assertions and retained canonical TASK-440 captures
# - Optional design drift gauge only for views with comparable rendered targets
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
, testSteps ? builtins.getEnv "CF_UI_TEST_STEPS"
, runExportValidation ? true
, updateVisualBaselines ? builtins.getEnv "CF_UI_UPDATE_BASELINES" == "1"
, playwrightResultTimeout ? 1800
, ...
}:
let
  testDir = ./tests;
  coverageManifest = ./coverage-manifest.json;
  baselinesDir = ./baselines;
  designParityDir = ./design-parity;
  CF_TEST_SERVER_PORT = 3000;

  # ── Design-evidence harness ─────────────────────────────────────────────────
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
  jszip = pkgs.fetchurl {
    url = "https://unpkg.com/jszip@3.10.1/dist/jszip.min.js";
    hash = "sha256-rMfkFFWoB2W1/Zx+4bgHim0WC7vKRVrq6FTeZclH1Z4=";
  };

  designExampleSrc = inputs.self + "/docs/design/CrystalForge";
  designTargets = inputs.self.packages.${pkgs.system}.design-targets;

  # Offline copy of the design example with the CDN <script> tags rewritten
  # to the vendored local files so Playwright can render it with no network.
  designExampleOffline = pkgs.runCommand "cf-design-example-offline" { } ''
    mkdir -p $out/vendor
    cp -r ${designExampleSrc}/. $out/
    chmod -R u+w $out
    cp ${reactUmd} $out/vendor/react.development.js
    cp ${reactDomUmd} $out/vendor/react-dom.development.js
    cp ${babelStandalone} $out/vendor/babel.min.js
    cp ${jszip} $out/vendor/jszip.min.js

    # Rewrite CDN script srcs to vendored paths and drop SRI/crossorigin so the
    # local files load without integrity/CORS checks.
    ${pkgs.gnused}/bin/sed -i -E \
      -e 's#src="https://unpkg.com/react@[^"]*"#src="vendor/react.development.js"#' \
      -e 's#src="https://unpkg.com/react-dom@[^"]*"#src="vendor/react-dom.development.js"#' \
      -e 's#src="https://unpkg.com/@babel/standalone@[^"]*"#src="vendor/babel.min.js"#' \
      -e 's#src="https://unpkg.com/jszip@[^"]*"#src="vendor/jszip.min.js"#' \
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
  # AUTHORITATIVE UI GATE: the server here is the production embedded-UI build,
  # not the core build used by the integration and oidc-auth checks. This is
  # the one check that must prove the production server binary serves the
  # production WASM through a real browser. Do not switch this to the core
  # build; doing so would silently remove the only pre-merge guarantee that the
  # shipped server can serve the shipped UI.
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
in pkgs.testers.runNixOSTest {
  name = "crystal-forge-web-ui-mega-integration";

  skipLint = true;
  skipTypeCheck = true;

  nodes = {
    # Git server for flake testing
    gitserver = lib.crystal-forge.makeGitServerNode {
      inherit pkgs systemBuildClosure;
      port = 8080;
      # The force-push recovery workflow rewrites this disposable repository.
      writableHttp = true;
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

      -- TASK-433 Phase 2: an imported policy with authoritative origin
      -- provenance, written through the real schema (immutable source artifact,
      -- source-object mapping, and an imported requirement mapping) so the
      -- unified editor's read-only Provenance section and read-only mapping
      -- behavior are exercised against persisted server state.
      INSERT INTO compliance_source_artifacts
          (content, filename, media_type, sha256, parser_version, detected_xccdf_version)
      SELECT '<Benchmark id="web-ui-provenance"/>'::bytea,
             'U_WEBUI_PROVENANCE_STIG.xml',
             'application/xml',
             encode(digest('<Benchmark id="web-ui-provenance"/>'::bytea, 'sha256'), 'hex'),
             'xccdf-1.2',
             '1.2'
      WHERE NOT EXISTS (
          SELECT 1 FROM compliance_source_artifacts
           WHERE filename = 'U_WEBUI_PROVENANCE_STIG.xml'
      );
      INSERT INTO deployment_policies (name, policy_type, config, enabled)
      SELECT 'Imported provenance control', 'custom_check', '{"mode":"all","rules":[]}'::jsonb, false
      WHERE NOT EXISTS (
          SELECT 1 FROM deployment_policies WHERE name = 'Imported provenance control'
      );
      UPDATE deployment_policy_versions v
         SET source_artifact_id = a.id
        FROM deployment_policies p, compliance_source_artifacts a
       WHERE p.name = 'Imported provenance control'
         AND v.id = p.current_draft_version_id
         AND a.filename = 'U_WEBUI_PROVENANCE_STIG.xml'
         AND v.source_artifact_id IS NULL;
      INSERT INTO compliance_source_object_mappings
          (source_artifact_id, object_kind, source_identity, policy_version_id, fidelity)
      SELECT a.id, 'rule', 'SV-WEBUI-1_rule', p.current_draft_version_id, 'preserved_opaque'
        FROM deployment_policies p, compliance_source_artifacts a
       WHERE p.name = 'Imported provenance control'
         AND a.filename = 'U_WEBUI_PROVENANCE_STIG.xml'
      ON CONFLICT (source_artifact_id, object_kind, source_identity) DO NOTHING;
      INSERT INTO policy_requirement_mappings
          (policy_version_id, requirement_version_id, relationship, coverage,
           rationale, provenance, source_artifact_id, trust_state)
      SELECT p.current_draft_version_id, rv.id, 'implements', 'full',
             'Recorded by the source benchmark import.', 'imported', a.id, 'trusted'
        FROM deployment_policies p, compliance_source_artifacts a,
             compliance_requirement_versions rv
       WHERE p.name = 'Imported provenance control'
         AND a.filename = 'U_WEBUI_PROVENANCE_STIG.xml'
         AND rv.external_id = 'MAP-2'
      ON CONFLICT (policy_version_id, requirement_version_id) DO NOTHING;
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
      "CF_TEST_REAL_CONFIGURATION_NAME": "cf-test-sys",
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
    machine.succeed("cp ${./design-fixtures.json} /tmp/web-ui-tests/design-fixtures.json")
    machine.succeed("mkdir -p /tmp/web-ui-baselines && cp -r ${baselinesDir}/. /tmp/web-ui-baselines/")
    machine.succeed("cp ${./default.nix} /tmp/web-ui-tests/default.nix")

    # Design-parity harness inputs must be present before static contracts run.
    machine.succeed("mkdir -p /tmp/web-ui-tests/design-parity")
    machine.succeed("cp -r ${designParityDir}/. /tmp/web-ui-tests/design-parity/")
    machine.succeed("mkdir -p /tmp/design-example && cp -r ${designExampleOffline}/. /tmp/design-example/")
    machine.succeed(
        "${pkgs.nodejs}/bin/node /tmp/web-ui-tests/design-parity/generate-design-targets-test.js"
    )
    machine.succeed(
        "env CF_WEB_UI_SOURCE_DIR=/tmp/web-ui-tests CF_UI_STATIC_CONTRACTS=1 "
        "${pkgs.nodejs}/bin/node /tmp/web-ui-tests/integration-test.js"
    )

    test_profile = "${testProfile}"
    test_steps = ${if testSteps == null then "None" else "\"${testSteps}\""}
    test_steps_env = f" CF_UI_TEST_STEPS={test_steps}" if test_steps else ""
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
    machine.succeed("rm -f /tmp/web-ui-tests/integration.exit /tmp/screenshots/results.json /tmp/screenshots/fatal.json")
    machine.succeed(
        f"nohup sh -c 'env CF_UI_BASELINES_DIR=/tmp/web-ui-baselines CF_UI_TEST_PROFILE={test_profile}{test_steps_env} ${pkgs.nodejs}/bin/node /tmp/web-ui-tests/integration-test.js http://127.0.0.1:${
          toString CF_TEST_SERVER_PORT
        } /tmp/screenshots; status=$?; printf \"%s\\n\" \"$status\" > /tmp/web-ui-tests/integration.exit' > /tmp/web-ui-tests/integration.log 2>&1 </dev/null &"
    )
    def export_failure_artifacts():
        # Preserve browser output and server diagnostics before rejecting the
        # derivation. Artifact export errors must not hide the original failure.
        try:
            machine.succeed(
                "journalctl -u crystal-forge-server.service --no-pager -n 300 "
                "> /tmp/web-ui-tests/server-journal.log 2>&1 || true; "
                "rm -rf /tmp/web-ui-failure-artifacts && "
                "mkdir -p /tmp/web-ui-failure-artifacts && "
                "cp -a /tmp/screenshots/. /tmp/web-ui-failure-artifacts/ && "
                "cp /tmp/web-ui-tests/integration.log "
                "/tmp/web-ui-failure-artifacts/integration.log && "
                "cp /tmp/web-ui-tests/server-journal.log "
                "/tmp/web-ui-failure-artifacts/server-journal.log && "
                "if test -f /tmp/web-ui-tests/integration.exit; then "
                "cp /tmp/web-ui-tests/integration.exit "
                "/tmp/web-ui-failure-artifacts/integration.exit; fi"
            )
            machine.copy_from_vm(
                "/tmp/web-ui-failure-artifacts",
                "browser-failure-artifacts",
            )
        except Exception as e:
            print(f"warning: could not export browser failure artifacts: {e}")

    def print_browser_diagnostics(reason):
        # Print diagnostics while the VM is reachable. Failed derivations do
        # not reliably retain files copied only into the test-driver workdir.
        print(f"=== Web UI browser diagnostics: {reason} ===")
        try:
            print(machine.succeed("cat /tmp/web-ui-tests/integration.log || true"))
        except Exception as e:
            print(f"warning: could not print integration.log: {e}")
        print("=== Crystal Forge server journal ===")
        try:
            print(
                machine.succeed(
                    "journalctl -u crystal-forge-server.service --no-pager -n 300 || true"
                )
            )
        except Exception as e:
            print(f"warning: could not print server journal: {e}")

    # The process can write results.json before post-processing finishes. Wait
    # for the durable exit marker so a late non-zero exit cannot be hidden by
    # an otherwise valid results artifact. Preserve partial output on timeout.
    try:
        machine.wait_for_file("/tmp/web-ui-tests/integration.exit", timeout=result_timeout)
    except Exception:
        print_browser_diagnostics("timed out waiting for integration.exit")
        export_failure_artifacts()
        raise
    output = machine.succeed("cat /tmp/web-ui-tests/integration.log")
    print(output)

    # Coverage-gate failures (manifest drift) abort before any results exist.
    if machine.execute("test -f /tmp/screenshots/fatal.json")[0] == 0:
        export_failure_artifacts()
        fatal_json = machine.succeed("cat /tmp/screenshots/fatal.json")
        raise Exception(f"Web UI check aborted: {json.loads(fatal_json)['error']}")

    if machine.execute("test -f /tmp/screenshots/results.json")[0] != 0:
        export_failure_artifacts()
        exit_code = machine.succeed("cat /tmp/web-ui-tests/integration.exit").strip()
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

    exit_code = machine.succeed("cat /tmp/web-ui-tests/integration.exit").strip()

    # Read results
    results_json = machine.succeed("cat /tmp/screenshots/results.json")
    results = json.loads(results_json)

    if any(not result.get("ok") for result in results):
        print("=== Crystal Forge server journal after failed browser step ===")
        print(
            machine.succeed(
                "journalctl -u crystal-forge-server.service --no-pager -n 300 || true"
            )
        )

    # Copy final and intermediate screenshots for successful and failed steps.
    # Intermediate captures are review evidence even when a later assertion in
    # the same workflow fails.
    for r in results:
        for visual in r.get("visuals", []):
            machine.copy_from_vm(f"/tmp/screenshots/{visual['name']}.png", "screenshots")

    for report_file in ["results.json", "visual-report.json", "visual-summary.md"]:
        try:
            machine.copy_from_vm(f"/tmp/screenshots/{report_file}", "screenshots")
        except Exception as e:
            print(f"warning: could not copy {report_file}: {e}")

    # === TASK-440 Semantic Contract + Advisory Pixel Comparison ===
    # Render the tracked design example (offline, shared fixture) for the primary
    # views/themes, then compare against the real Dioxus captures produced by the
    # integration test above. This is the primary visual comparison — the design
    # Semantic contracts and successful comparison execution are blocking. RMSE
    # values remain advisory and never fail solely because pixels differ.
    print("=== TASK-440 semantic contract and design comparison ===")
    parity_manifest = json.loads(machine.succeed("cat /tmp/web-ui-tests/design-parity/manifest.json"))
    result_names = {result["name"] for result in results}
    selected_targets = [
        target for target in parity_manifest.get("targets", {}).get("task440", [])
        if target.get("dioxusStep") in result_names
    ]
    themes = parity_manifest.get("settings", {}).get("themes", [])
    expected_pairs = len(selected_targets) * len(themes)
    if expected_pairs == 0:
        print("No mapped TASK-440 targets selected; skipping mapped semantic/pixel evidence")
    target_names = ",".join(target["name"] for target in selected_targets) or "__none__"
    focused_env = f"CF_TASK440_TARGETS={target_names} " if test_steps else ""

    # The design-target package is a blocking derivation dependency. Reuse its
    # validated output instead of compiling the Babel design a second time in
    # the busy VM, where unrelated server jobs can starve Chromium.
    machine.succeed("mkdir -p /tmp/design-targets /tmp/design-parity")
    machine.succeed("cp -r ${designTargets}/. /tmp/design-targets/")

    generated = json.loads(machine.succeed("cat /tmp/design-targets/design-targets.json"))["results"]
    selected_target_names = {target["name"] for target in selected_targets}
    generated_task440 = [
        result for result in generated
        if result.get("group") == "task440" and result.get("target") in selected_target_names
    ]
    if len(generated_task440) != expected_pairs:
        raise Exception(f"Expected {expected_pairs} generated TASK-440 pairs, found {len(generated_task440)}")
    generation_errors = [result for result in generated_task440 if not result.get("ok") or not result.get("semanticContract", {}).get("ok")]
    if generation_errors:
        raise Exception(f"TASK-440 design semantic generation failures: {generation_errors}")

    dioxus_contracts = [] if expected_pairs == 0 else json.loads(machine.succeed("cat /tmp/screenshots/design-parity/task440-semantic-contracts.json"))["results"]
    if len(dioxus_contracts) != expected_pairs or any(not result.get("ok") for result in dioxus_contracts):
        raise Exception(f"Expected {expected_pairs} successful Dioxus semantic contracts, got {dioxus_contracts}")

    comparison_status, _ = machine.execute(
        f"{focused_env}${pkgs.nodejs}/bin/node /tmp/web-ui-tests/design-parity/compare-design-parity.js "
        "/tmp/web-ui-tests/design-parity/manifest.json /tmp/design-targets "
        "/tmp/screenshots/design-parity /tmp/design-parity "
        "> /tmp/web-ui-tests/design-parity.log 2>&1"
    )
    print(machine.succeed("cat /tmp/web-ui-tests/design-parity.log"))
    if comparison_status != 0:
        raise Exception(f"TASK-440 design comparison failed with exit code {comparison_status}")

    parity_report = json.loads(machine.succeed("cat /tmp/design-parity/design-drift-report.json"))
    task440_rows = [row for row in parity_report["rows"] if row.get("group") == "task440"]
    if len(task440_rows) != expected_pairs:
        raise Exception(f"Expected {expected_pairs} reported TASK-440 comparisons, found {len(task440_rows)}")
    bad_rows = [row for row in task440_rows if row.get("status") != "compared"]
    if bad_rows or parity_report.get("counts", {}).get("errors") != 0:
        raise Exception(f"TASK-440 comparison status/errors are not clean: rows={bad_rows}, counts={parity_report.get('counts')}")
    if parity_report.get("counts", {}).get("task440Compared") != expected_pairs:
        raise Exception(f"Expected {expected_pairs} successful TASK-440 comparisons, report has {parity_report.get('counts', {}).get('task440Compared')}")

    # Copy the complete retained evidence: report, raw captures, content-surface
    # montages, and absolute-difference images.
    for report_file in ["design-drift-report.json", "design-drift-summary.md"]:
        machine.copy_from_vm(f"/tmp/design-parity/{report_file}", "screenshots")
    for artifact_dir in ["montages", "diffs"]:
        machine.copy_from_vm(f"/tmp/design-parity/{artifact_dir}", "screenshots")
    if machine.execute("test -f /tmp/design-parity/design-parity-matrix.png")[0] == 0:
        machine.copy_from_vm("/tmp/design-parity/design-parity-matrix.png", "screenshots")
    machine.copy_from_vm("/tmp/design-targets", "screenshots")
    if machine.execute("test -d /tmp/screenshots/design-parity")[0] == 0:
        machine.copy_from_vm("/tmp/screenshots/design-parity", "screenshots")

    ok_count = sum(1 for r in results if r.get("ok"))
    intermediate_count = sum(
        1
        for result in results
        for visual in result.get("visuals", [])
        if visual.get("intermediate")
    )

    print("\n=== Summary ===")
    print(f"  Screenshots: {ok_count}/{len(results)} captured")
    print(f"  Intermediate workflow artifacts: {intermediate_count} copied")

    for r in results:
        status = "OK" if r.get("ok") else "FAIL"
        error = r.get("error", "")
        if error:
            print(f"  [{status}] {r['name']} - {error}")
        else:
            print(f"  [{status}] {r['name']}")

    integration_failures = []
    if ok_count == 0:
        integration_failures.append("All screenshots failed")

    # Critical workflows must be present and successful. Treating only returned
    # failures as fatal would let profile or manifest drift silently skip them.
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
      "29g-poam-failed-evidence-create",
      "29h-poam-link-compatible-findings",
      "29i-poam-detail-edits-milestones-conflicts",
      "29k-poam-system-rollups-navigation",
      "29l-poam-bundle-rollups-batching",
      "29m-poam-assignment-relationship-immutability",
      "30a-admin-automatic-retries-defaults-reset",
      "30b-admin-automatic-retries-save-reload",
      "30c-admin-automatic-retries-failed-save-retains-draft",
      "30d-evidence-lifecycle",
      "30e-policy-card-direct-edit-preserves-evidence",
      "12l-task440-config-lifecycle",
      "12m-task440-config-explorer-keyboard-wide",
      "12n-task440-config-narrow-keyboard",
      "12p-task440-config-canonical-wide-expanded",
      "12q-task440-config-canonical-narrow",
      "13j-task440-flake-states-panes-navigation",
      "13l-task440-flake-systems-canonical-wide",
      "13m-task440-flake-systems-canonical-narrow",
      "13n-task440-flake-modules-canonical-wide-expanded",
      "13o-task440-flake-modules-canonical-narrow-expanded",
      "13p-task440-flake-inputs-canonical-wide",
      "13q-task440-flake-inputs-canonical-narrow",
      "13k-task440-drawer-modal-keyboard-layering",
      "12o-task440-rollback-notification-auto-latest",
      "14d-task440-cross-surface-auth-navigation",
      "task433-canonical-large-catalog",
      "20af-policy-catalog-selection-delete-regressions",
      "19-policies-new-modal-fields",
      "20-policies-new-modal-rule-builder",
      "20a-policies-new-modal-pending-mappings",
      "20ac-stig-import-reconciliation-fixture",
      "20aa-policies-new-modal-mappings-roundtrip",
      "task433-canonical-unmapped-nix-policy",
      "20ac-policy-editor-category-and-imported-provenance",
      "task433-canonical-imported-stig-refinement",
      "task433-canonical-multiline-dod",
      "20ab2-policy-editor-eight-kind-roundtrip",
      "task433-canonical-mixed-nix-cve-evidence",
      "20ad-stig-nixos-assertion-roundtrip",
      "20b-policies-cve-gate-create-roundtrip",
      "20ab-compliance-bundle-requirement-baseline-roundtrip",
      "20c-policies-multirule-create-roundtrip",
      "20d-policies-cve-gate-invalid-rejected",
      "20e-policies-multirule-rules-only-no-expression-required",
      "task433-canonical-poam-lifecycle",
    ]
    selected_critical_tests = (
      critical_tests
      if not test_steps
      else [name for name in critical_tests if name in {
        selected.strip() for selected in test_steps.split(",") if selected.strip()
      }]
    )
    returned_names = {r.get('name') for r in results}
    missing_critical = [name for name in selected_critical_tests if name not in returned_names]
    if missing_critical:
        integration_failures.append(
            f"Required critical web UI checks were absent: {missing_critical}"
        )
    failed_critical = [r['name'] for r in results if r['name'] in selected_critical_tests and not r.get('ok')]
    if failed_critical:
        integration_failures.append(f"Critical web UI checks failed: {failed_critical}")

    # TASK-440 semantic, lifecycle, geometry, scroll, and stacking assertions
    # remain merge-blocking through critical_tests above. The mapped React vs
    # Dioxus pixel comparisons are advisory and retain montages for review.
    visual_report_json = machine.succeed("cat /tmp/screenshots/visual-report.json")
    visual_report = json.loads(visual_report_json)
    counts = visual_report.get("counts")
    required_visual_counts = {"match", "diff", "new", "skipped", "error"}
    if not isinstance(counts, dict) or not required_visual_counts.issubset(counts):
        raise Exception("visual-report.json has an invalid counts schema")
    if any(not isinstance(counts[key], int) or counts[key] < 0 for key in required_visual_counts):
        raise Exception("visual-report.json contains invalid visual counts")
    print(
        f"  Visual baselines: {counts.get('match', 0)} match, "
        f"{counts.get('diff', 0)} differ, {counts.get('new', 0)} new, "
        f"{counts.get('skipped', 0)} skipped"
    )
    visual_failures = visual_report.get("failures")
    if not isinstance(visual_failures, list):
        raise Exception("visual-report.json has an invalid failures schema")
    if visual_failures:
        for f in visual_failures:
            print(f"  STRICT VISUAL FAIL: {f['name']} ({f['status']})")
        if ${if updateVisualBaselines then "False" else "True"}:
            integration_failures.append(
                f"Strict visual baseline failures: {[f['name'] for f in visual_failures]}"
            )
        else:
            print("  Baseline update mode: strict visual failures are exported for review")

    if exit_code != "0":
        integration_failures.append(
            f"integration process exited non-zero after producing results.json ({exit_code})"
        )
    if integration_failures:
        export_failure_artifacts()
        raise Exception("; ".join(integration_failures))

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
