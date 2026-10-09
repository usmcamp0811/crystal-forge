{inputs, lib, pkgs, ...}: let
  credentials = lib.crystal-forge.makeNiks3TestCredentials {inherit pkgs; productionPki = true;};
  productionWritePki = credentials.productionWritePki;
  evaluatorNix = pkgs.nix-eval-jobs.nix;
  variants = ["token-public" "mtls-public" "token-private" "mtls-private" "token-public-proxy" "mtls-split"];
  makeTarget = name: builtins.derivation {
    name = "niks3-${name}";
    system = pkgs.stdenv.hostPlatform.system;
    builder = "${pkgs.busybox}/bin/sh";
    args = ["-c" ''
      ${pkgs.busybox}/bin/mkdir -p "$out/bin" "$out/sw"
      ${pkgs.busybox}/bin/printf '#!${pkgs.busybox}/bin/sh\nexit 0\n' > "$out/bin/switch-to-configuration"
      ${pkgs.busybox}/bin/chmod +x "$out/bin/switch-to-configuration"
      ${lib.optionalString (name == "mtls-split") ''
        # A real upload workload makes short-lived CLI argument/environment
        # inspection practical without artificial sleeps or provider delays.
        ${pkgs.busybox}/bin/dd if=/dev/urandom of="$out/split-pki-workload" bs=1048576 count=16 2>/dev/null
      ''}
    ''];
  };
  targets = lib.genAttrs variants makeTarget;
  negativeTargets = lib.genAttrs ["trust-a-only" "trust-b-only"] makeTarget;
  # Public, deterministic Ed25519 fixtures. The shared test-keys directory
  # contains placeholder strings, not an authentication-capable keypair.
  key = pkgs.writeText "niks3-agent.key" "+/GIbrjuyb3Hf2es5w+vWSlDUhEsAIojiyyfgskC7QA=\n";
  pub = "DpOiy7W+DqZEg3KR0fvP5Q8k4FR4K1NB+qyYQLxhnFc=";
  common = {nodes, ...}: {
    imports = [inputs.self.nixosModules.crystal-forge];
    networking.firewall.enable = false;
    # A 9p host-store mount would expose outputs built as driver dependencies.
    # Closure-only images make absence checks prove actual cache downloads.
    virtualisation = {writableStore = true; useNixStoreImage = true; memorySize = 4096; cores = 2; diskSize = 8192;};
    nix.settings = {experimental-features = ["nix-command" "flakes"]; substituters = lib.mkForce [];};
    security.pki.certificateFiles = ["${credentials}/ca.crt"];
    environment.etc."niks3-fixtures".source = credentials;
    environment.etc."niks3-production-pki".source = "${credentials}/production";
    networking.hosts.${nodes.production.networking.primaryIPAddress} = ["push-cache.test" "s3-cache.test" "read-cache.test" "read-foreign.test"];
    environment.etc."agent.key".source = key;
    # Inspect protected responses and generated TOML inside the isolated guests.
    # Trust tests use the production loader without environment overrides.
    environment.systemPackages = [evaluatorNix pkgs.curl pkgs.jq pkgs.busybox pkgs.python3];
    services.crystal-forge = {
      enable = true;
      client.enable = false;
      server.enable = false;
      build.enable = false;
    };
  };
in pkgs.testers.runNixOSTest {
  name = "crystal-forge-niks3-cache";
  skipLint = true;
  skipTypeCheck = true;
  globalTimeout = 1800;
  nodes = {
    cache = {
      imports = [(lib.crystal-forge.makeNiks3CacheNode {inherit pkgs credentials;})];
      # Cleanup diagnostics run inside each guest without exporting raw /proc.
      environment.systemPackages = [pkgs.python3];
    };
    production = {
      imports = [(lib.crystal-forge.makeNiks3CacheNode {inherit pkgs credentials; productionPki = true; basicRead = true;})];
      networking.hosts."127.0.0.1" = ["push-cache.test" "s3-cache.test" "read-cache.test" "read-foreign.test"];
    };
    server = {lib, ...}: {
      imports = [common];
      security.pki.certificateFiles = [productionWritePki.serverB];
      virtualisation.additionalPaths = [pkgs.busybox] ++ map (target: target.drvPath) (builtins.attrValues (targets // negativeTargets));
      services.crystal-forge = {
        local-database = true;
        database = {host = "127.0.0.1"; user = "crystal_forge"; name = "crystal_forge";};
        server = {
          enable = lib.mkForce true;
          package = pkgs.crystal-forge.default.cf-server-core-drv;
          # Direct VM HTTP is an authenticated but unverified transport probe.
          # Only nginx's loopback peer can attest confidential HTTPS delivery.
          host = "0.0.0.0";
          port = 8000;
          trust_forwarded_builder_https = true;
          trustedProxyCidrs = ["127.0.0.1/32"];
          allow_private_cache_test_targets = true;
        };
        build.remote_execution_strategy = "server_derivation";
        vulnix = {poll_interval = "2s"; timeout = "15s"; max_retries = 0;};
        cache = {push_after_build = false; encryption_key_file = "/etc/cache-encryption-key";};
        flakes.watched = [];
        environments = [
          {name = "niks3"; description = "Isolated Niks3 tests"; is_active = true; risk_profile = "LOW"; compliance_level = "NONE";}
          {name = "unrelated"; description = "Credential isolation sentinel"; is_active = true; risk_profile = "LOW"; compliance_level = "NONE";}
        ];
        systems = [{hostname = "agent"; public_key = pub; environment = "niks3";}];
      };
      environment.etc."cache-encryption-key".text = "niks3-vm-only-cache-encryption-key-470";
      services.postgresql.authentication = lib.mkForce ''
        local all postgres trust
        local all all peer
        host all all 127.0.0.1/32 trust
        host all postgres 10.0.2.2/32 trust
      '';
      services.postgresql.settings.listen_addresses = lib.mkForce "*";
      # The server initializes the schema on first start. The unrelated SQL
      # maintenance timer must not race that initialization in this fixture.
      systemd.services.crystal-forge-postgres-jobs.wantedBy = lib.mkForce [];
      systemd.timers.crystal-forge-postgres-jobs.wantedBy = lib.mkForce [];
      services.nginx = {
        enable = true;
        virtualHosts.control = {
          addSSL = true;
          listen = [{addr = "0.0.0.0"; port = 443; ssl = true;}];
          sslCertificate = "${credentials}/server.crt";
          sslCertificateKey = "${credentials}/server.key";
          locations."/" = {
            proxyPass = "http://127.0.0.1:8000";
            extraConfig = ''proxy_set_header X-Forwarded-Proto https;'';
          };
        };
      };
    };
    builder = {lib, ...}: {
      imports = [common];
      security.pki.certificateFiles = [productionWritePki.serverB];
      # The service has its own runtime PATH. Negative CLI assertions run in
      # the driver shell and must use the same packaged CLI, not exit 127.
      environment.systemPackages = [pkgs.crystal-forge.default.niks3];
      services.crystal-forge.build = {
        enable = lib.mkForce true;
        api_mode = true;
        api_key_file = key;
        server_url = "https://server";
        poll_interval = "2s";
        supported_execution_strategies = ["server_derivation"];
        remote_execution_strategy = "server_derivation";
        # This check exercises the server-local CVE materialization path.
        # Keep the remote worker from racing the server for that scan lease.
        cve_scanning_enabled = false;
      };
      # Register the identity before starting this API-only worker in the test.
      systemd.services.crystal-forge-builder.wantedBy = lib.mkForce [];
    };
    agent = {lib, ...}: {
      imports = [common];
      # Basic reads use system trust under B, independently of write mTLS.
      security.pki.certificateFiles = [productionWritePki.serverB];
      services.crystal-forge.client = {
        enable = lib.mkForce true;
        server_host = "server";
        server_port = 443;
        private_key = "/etc/agent.key";
      };
      # Each variant deliberately restarts the agent to trigger a signed
      # heartbeat. Do not defer that test request until its next 600s heartbeat.
      # The module's optional deployment TOML currently serializes its poll
      # duration as a string although cf-config expects integer seconds.
      # Use the supported environment layer for this fixture-only override.
      # DeploymentConfig requires a complete section when any field is set.
      systemd.services.crystal-forge-agent.environment = {
        CRYSTAL_FORGE__DEPLOYMENT__MAX_DEPLOYMENT_AGE_MINUTES = "30";
        CRYSTAL_FORGE__DEPLOYMENT__DRY_RUN_FIRST = "true";
        CRYSTAL_FORGE__DEPLOYMENT__FALLBACK_TO_LOCAL_BUILD = "false";
        CRYSTAL_FORGE__DEPLOYMENT__DEPLOYMENT_TIMEOUT_MINUTES = "60";
        CRYSTAL_FORGE__DEPLOYMENT__DEPLOYMENT_POLL_INTERVAL = "900";
        CRYSTAL_FORGE__DEPLOYMENT__POST_AGENT_START_DEPLOYMENT_DELAY = "0";
        CRYSTAL_FORGE__DEPLOYMENT__REQUIRE_SIGS = "true";
      };
      systemd.services.crystal-forge-agent.wantedBy = lib.mkForce [];
      # The production module creates this tmpfiles owner only for server or
      # builder nodes. The standalone agent fixture still needs that owner.
      users.users.crystal-forge = {isSystemUser = true; group = "crystal-forge";};
    };
  };
  extraPythonPackages = p: [p.pytest p.psycopg2 p.pynacl p.cryptography pkgs.crystal-forge.cf-test-suite];
  testScript = ''
    from cf_test.tests.cache.test_niks3_cache import run_matrix
    start_all()
    cache.wait_for_unit("niks3.service")
    cache.wait_for_unit("nginx.service")
    cache.wait_for_open_port(5751)
    production.wait_for_unit("niks3-production.service")
    production.wait_for_unit("niks3-production-bridge.service")
    production.wait_for_open_port(5754)
    server.wait_for_unit("crystal-forge-server.service")
    server.wait_for_open_port(8000)
    server.wait_for_unit("nginx.service")
    run_matrix(
      {"server": server, "builder": builder, "agent": agent, "cache": cache, "production": production},
      ${builtins.toJSON (lib.mapAttrs (_: target: {drv = target.drvPath; out = builtins.unsafeDiscardStringContext target.outPath;}) targets)},
      "${pub}",
      "${credentials}",
      ${builtins.toJSON (lib.mapAttrs (_: target: {drv = target.drvPath; out = builtins.unsafeDiscardStringContext target.outPath;}) negativeTargets)},
    )
  '';
} // {
  # Infrastructure-only diagnosis remains available while a shared Rust build
  # is incomplete. It cannot satisfy the remote-builder/agent acceptance gate.
  fixture = pkgs.testers.runNixOSTest {
    name = "niks3-cache-fixture";
    skipLint = true;
    skipTypeCheck = true;
    nodes = {
      cache = lib.crystal-forge.makeNiks3CacheNode {inherit pkgs credentials;};
      probe = {
        networking.firewall.enable = false;
        virtualisation.writableStore = true;
        virtualisation.additionalPaths = [pkgs.busybox targets.token-public.drvPath];
        security.pki.certificateFiles = ["${credentials}/ca.crt"];
        environment.systemPackages = [evaluatorNix pkgs.crystal-forge.default.niks3 pkgs.curl pkgs.jq];
        environment.etc."niks3-fixtures".source = credentials;
        nix.settings.experimental-features = ["nix-command" "flakes"];
      };
    };
    testScript = ''
      start_all()
      cache.wait_for_unit("niks3.service")
      cache.wait_for_unit("nginx.service")
      probe.succeed("command -v niks3; niks3 --help >/dev/null")
      probe.succeed("curl --fail --silent https://cache:5751/api/cache-config | jq -e '.public_keys | length == 2'")
      probe.succeed("nix-store --realise ${targets.token-public.drvPath}")
      probe.succeed("umask 077; printf '%s' niks3-nonproduction-static-token-470-00000000 > /tmp/token")
      probe.succeed("niks3 push --server-url https://cache:5751 --auth-token-path /tmp/token ${targets.token-public.outPath} >/dev/null 2>&1")
      probe.succeed("niks3 push --server-url https://cache:5751 --client-cert /etc/niks3-fixtures/write.crt --client-key /etc/niks3-fixtures/write.key ${targets.token-public.outPath} >/dev/null 2>&1")
      probe.succeed("umask 077; printf '%s' invalid-niks3-token-470-00000000000000000 > /tmp/invalid-token")
      probe.fail("niks3 push --server-url https://cache:5751 --auth-token-path /tmp/invalid-token ${targets.token-public.outPath} >/dev/null 2>&1")
      probe.fail("niks3 push --server-url https://cache:5751 --client-cert /etc/niks3-fixtures/read.crt --client-key /etc/niks3-fixtures/read.key ${targets.token-public.outPath} >/dev/null 2>&1")
      probe.succeed("curl --fail --silent https://cache:5753/nix-cache-info")
      probe.fail("curl --fail --silent https://cache:5752/nix-cache-info")
      probe.fail("curl --fail --silent --cert /etc/niks3-fixtures/wrong.crt --key /etc/niks3-fixtures/wrong.key https://cache:5752/nix-cache-info")
      probe.succeed("curl --fail --silent --cert /etc/niks3-fixtures/read.crt --key /etc/niks3-fixtures/read.key https://cache:5752/nix-cache-info")
      # A separate local store forces a real download and signature check.
      # Test each signing key independently, not only the narinfo text.
      for index in (0, 1):
        probe.succeed(f"nix copy --from 'https://cache:5752?tls-certificate=/etc/niks3-fixtures/read.crt&tls-private-key=/etc/niks3-fixtures/read.key' --to 'local?root=/tmp/read-{index}' --option trusted-public-keys \"$(cat /etc/niks3-fixtures/signing-{index}.pub)\" --option require-sigs true ${targets.token-public.outPath}")
      probe.fail("nix copy --from https://cache:5751 --to 'local?root=/tmp/wrong-signature' --option trusted-public-keys 'wrong:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=' --option require-sigs true ${targets.token-public.outPath}")
      probe.succeed("rm /tmp/token /tmp/invalid-token")
      import base64
      logs = cache.succeed("journalctl --no-pager -u niks3 -u niks3-secrets -u garage-setup -u nginx")
      assert "niks3-nonproduction-static-token-470-00000000" not in logs
      assert "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" not in logs
      for index in (2, 3):
        payload = base64.b64encode(bytes.fromhex("302e020100300506032b657004220420") + bytes([index + 1]) * 32).decode()
        assert payload not in logs, "Client private key leaked in fixture service logs"
    '';
  };
  production-fixture = pkgs.testers.runNixOSTest {
    name = "niks3-production-pki-fixture";
    skipLint = true;
    skipTypeCheck = true;
    nodes = {
      production = {
        imports = [(lib.crystal-forge.makeNiks3CacheNode {inherit pkgs credentials; productionPki = true;})];
        networking.hosts."127.0.0.1" = ["push-cache.test" "s3-cache.test" "read-cache.test"];
      };
      probe = {nodes, ...}: {
        networking.firewall.enable = false;
        networking.hosts.${nodes.production.networking.primaryIPAddress} = ["push-cache.test" "s3-cache.test" "read-cache.test"];
        virtualisation = {writableStore = true; additionalPaths = [pkgs.busybox targets.mtls-split.drvPath]; memorySize = 4096; cores = 2;};
        security.pki.certificateFiles = ["${credentials}/ca.crt" productionWritePki.serverB];
        environment.systemPackages = [evaluatorNix pkgs.crystal-forge.default.niks3 pkgs.curl pkgs.python3];
        environment.etc."niks3-production-pki".source = "${credentials}/production";
        nix.settings.experimental-features = ["nix-command" "flakes"];
      };
    };
    extraPythonPackages = p: [p.pytest p.psycopg2 p.pynacl p.cryptography pkgs.crystal-forge.cf-test-suite];
    testScript = ''
      from cf_test.tests.cache.test_niks3_cache import run_production_transport_fixture
      start_all()
      production.wait_for_unit("niks3-production.service")
      production.wait_for_unit("niks3-production-bridge.service")
      production.wait_for_open_port(5754)
      run_production_transport_fixture(production, probe,
        ${builtins.toJSON {drv = targets.mtls-split.drvPath; out = builtins.unsafeDiscardStringContext targets.mtls-split.outPath;}}, "${credentials}")
    '';
  };
}
