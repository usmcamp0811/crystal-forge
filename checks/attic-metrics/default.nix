{inputs, lib, pkgs, ...}: let
  # Both native packages come from the locked nixpkgs input. Fail evaluation if
  # its source changes without a deliberate schema-contract review.
  atticRevision = "12cbeca141f46e1ade76728bce8adc447f2166c6";
  python = pkgs.python3.withPackages (p: [p.psycopg p.requests]);
  credentials = lib.crystal-forge.makeNiks3TestCredentials {
    inherit pkgs;
    strictTls = true;
    extraDnsNames = ["attic-metrics.test"];
  };
  target = text: pkgs.runCommand "attic-metrics-native-${text}" {} ''
    printf '%s' '${text}' > "$out"
  '';
  first = target "AAAA";
  second = target "BBBB";
  common = {
    system.stateVersion = "26.05";
    virtualisation = {memorySize = 3072; cores = 2; writableStore = true; useNixStoreImage = true;};
    environment.systemPackages = [python pkgs.curl pkgs.postgresql pkgs.coreutils];
    environment.etc."attic-metrics-test.py".source = ./test.py;
    security.pki.certificateFiles = ["${credentials}/ca.crt"];
    nix.settings = {experimental-features = ["nix-command" "flakes"]; substituters = lib.mkForce [];};
  };
  mappings = map (entry: {
    destination_id = entry.id;
    url = "https://attic-metrics.test:9443/${entry.prefix or "inventory"}/";
    token_file = "/run/cf-metrics-tokens/${entry.file or entry.cache}";
  }) [
    {id = 4701; cache = "populated";}
    {id = 4702; cache = "empty";}
    {id = 4703; cache = "secondary";}
    {id = 4704; cache = "populated";}
    {id = 4706; cache = "populated"; file = "wrong";}
    {id = 4707; cache = "missing";}
    {id = 4708; cache = "deleted";}
    {id = 4709; cache = "populated"; prefix = "wrong-name";}
    {id = 4710; cache = "populated"; prefix = "missing-field";}
    {id = 4711; cache = "populated"; prefix = "overflow";}
    {id = 4712; cache = "populated"; prefix = "denied";}
    {id = 4713; cache = "populated"; file = "root-owned";}
    {id = 4714; cache = "shared";}
    {id = 4715; cache = "Team+1";}
    {id = 4716; cache = "minimum";}
  ];
in assert pkgs.attic-server.src.rev == atticRevision;
pkgs.testers.runNixOSTest {
  name = "crystal-forge-attic-metrics";
  skipLint = true;
  skipTypeCheck = true;
  globalTimeout = 1200;
  nodes = {
    attic = {nodes, ...}: {
      imports = [common inputs.self.nixosModules.attic-metrics];
      environment.systemPackages = [pkgs.attic-client pkgs.attic-server];
      environment.etc."publication-first".text = "${first}";
      environment.etc."publication-second".text = "${second}";
      users.users.attic = {isSystemUser = true; group = "attic";};
      users.groups.attic = {};
      services.postgresql = {
        enable = true;
        ensureDatabases = ["attic" "unrelated"];
        ensureUsers = [{name = "attic"; ensureDBOwnership = true;}];
        settings = {
          listen_addresses = lib.mkForce "127.0.0.1";
          # Intentional permission failures must not export SQL/private data.
          log_min_error_statement = "panic";
          log_min_messages = "panic";
        };
        authentication = lib.mkForce ''
          local all postgres trust
          local all all peer
          host attic cf_attic_metrics 127.0.0.1/32 scram-sha-256
          host attic attic 127.0.0.1/32 trust
        '';
      };
      environment.etc."atticd.toml".text = ''
        listen = "127.0.0.1:8080"
        api-endpoint = "http://127.0.0.1:8080/"
        [database]
        url = "postgresql://attic@127.0.0.1/attic"
        [storage]
        type = "local"
        path = "/var/lib/attic/storage"
        [chunking]
        nar-size-threshold = 65536
        min-size = 16384
        avg-size = 65536
        max-size = 262144
        [compression]
        type = "zstd"
        level = 8
        [jwt.signing]
        token-hs256-secret-base64 = "dGVzdCBzZWNyZXQgZm9yIGF0dGljZA=="
      '';
      systemd.services.atticd = {
        wantedBy = ["multi-user.target"];
        after = ["postgresql.service" "postgresql-setup.service"];
        requires = ["postgresql.service" "postgresql-setup.service"];
        environment.RUST_LOG = "error";
        serviceConfig = {
          ExecStart = "${pkgs.attic-server}/bin/atticd --config /etc/atticd.toml";
          User = "attic";
          Group = "attic";
          StateDirectory = "attic";
          WorkingDirectory = "/var/lib/attic";
        };
      };
      services.crystal-forge.attic-metrics = {
        enable = true;
        package = pkgs.crystal-forge.attic-metrics;
        databaseUriFile = "/run/attic-metrics-secrets/database";
        tokensFile = "/run/attic-metrics-secrets/tokens";
      };
      # The driver creates the restricted role and root-owned runtime sources
      # after native migrations/uploads. All credential copying and hardening
      # remain supplied by the actual optional module.
      systemd.services.cf-attic-metrics.wantedBy = lib.mkForce [];
      networking.firewall = {
        enable = true;
        # Only the CF VM can enter the private TLS proxy. Native HTTP, adapter
        # loopback and PostgreSQL have no external ingress.
        extraCommands = ''
          iptables -A nixos-fw -p tcp -s ${nodes.server.networking.primaryIPAddress} --dport 9443 -j nixos-fw-accept
        '';
      };
      services.nginx = {
        enable = true;
        mapHashBucketSize = 128;
        commonHttpConfig = ''
          map $http_authorization $metrics_token { default false; "Bearer ${builtins.concatStringsSep "" (lib.replicate 64 "a")}" true; }
          map $http_cookie $metrics_cookie { "" false; default true; }
          map $http_x_csrf_token $metrics_csrf { "" false; default true; }
          log_format metrics_safe escape=json '{"token":$metrics_token,"cookie":$metrics_cookie,"csrf":$metrics_csrf,"status":$status}';
        '';
        virtualHosts.metrics = {
          addSSL = true;
          serverName = "attic-metrics.test";
          listen = [{addr = "0.0.0.0"; port = 9443; ssl = true;}];
          sslCertificate = "${credentials}/server.crt";
          sslCertificateKey = "${credentials}/server.key";
          extraConfig = ''
            allow ${nodes.server.networking.primaryIPAddress};
            allow 127.0.0.1;
            deny all;
            access_log /var/log/nginx/metrics-safe.log metrics_safe;
            error_log /dev/null;
          '';
          locations."/inventory/" = {
            proxyPass = "http://127.0.0.1:9121/";
            extraConfig = ''proxy_set_header Cookie ""; proxy_set_header X-CSRF-Token "";'';
          };
          locations."/wrong-name/" = {
            extraConfig = ''default_type application/json;'';
            return = ''200 '{"schema_version":1,"cache":"other","cached_path_count":1,"logical_nar_bytes":100}' '';
          };
          locations."/missing-field/" = {
            extraConfig = ''default_type application/json;'';
            return = ''200 '{"schema_version":1,"cache":"populated","cached_path_count":1}' '';
          };
          locations."/overflow/" = {
            extraConfig = ''default_type application/json;'';
            return = ''200 '{"schema_version":1,"cache":"populated","cached_path_count":1,"logical_nar_bytes":18446744073709551616}' '';
          };
          locations."/denied/".return = ''403 'private-provider-error-marker' '';
        };
      };
    };
    server = {nodes, ...}: {
      imports = [common inputs.self.nixosModules.crystal-forge];
      networking.hosts.${nodes.attic.networking.primaryIPAddress} = ["attic-metrics.test"];
      networking.firewall.enable = true;
      services.crystal-forge = {
        enable = true;
        log_level = "error";
        local-database = true;
        database = {host = "127.0.0.1"; user = "crystal_forge"; name = "crystal_forge";};
        client.enable = false;
        build.enable = false;
        server = {
          enable = true;
          package = pkgs.crystal-forge.default.cf-server-core-drv;
          host = "127.0.0.1";
          port = 8000;
          auth_mode = "local";
          allow_private_cache_test_targets = true;
          attic_metrics_endpoints = mappings;
        };
        cache.push_after_build = false;
        flakes.watched = [];
      };
      systemd.services.crystal-forge-server.environment.AUTH_MODE = "local";
      systemd.services.crystal-forge-server.after = ["postgresql-setup.service"];
      systemd.services.crystal-forge-server.requires = ["postgresql-setup.service"];
      systemd.services.crystal-forge-postgres-jobs.wantedBy = lib.mkForce [];
      systemd.timers.crystal-forge-postgres-jobs.wantedBy = lib.mkForce [];
      services.postgresql = {
        settings = {
          listen_addresses = lib.mkForce "127.0.0.1";
          log_min_error_statement = "panic";
          log_min_messages = "panic";
        };
        authentication = lib.mkForce ''
          local all postgres trust
          local all all peer
          host all all 127.0.0.1/32 trust
        '';
      };
      systemd.services.metrics-token-fixture = {
        before = ["crystal-forge-server.service"];
        requiredBy = ["crystal-forge-server.service"];
        serviceConfig = {Type = "oneshot"; RemainAfterExit = true;};
        path = [python];
        script = "python /etc/attic-metrics-test.py cf-tokens";
      };
    };
  };
  testScript = ''
    import json
    start_all()
    attic.wait_for_unit("atticd.service")
    attic.wait_for_open_port(8080)
    attic.succeed("python /etc/attic-metrics-test.py attic-setup")
    attic.succeed("systemctl start cf-attic-metrics.service")
    attic.wait_for_open_port(9121, timeout=30)
    attic.succeed("python /etc/attic-metrics-test.py module-credentials")
    attic.wait_for_unit("nginx.service")
    attic.succeed("python /etc/attic-metrics-test.py attic-verify")
    expected = json.loads(attic.succeed("python /etc/attic-metrics-test.py expected"))
    server.succeed("python /etc/attic-metrics-test.py cf-setup")
    server.succeed("python /etc/attic-metrics-test.py cf-verify '" + json.dumps(expected) + "'")
    before = attic.succeed("python /etc/attic-metrics-test.py requests").strip()
    server.succeed("python /etc/attic-metrics-test.py cf-gates")
    after = attic.succeed("python /etc/attic-metrics-test.py requests").strip()
    assert before == after, "metadata gates contacted provider"
    attic.succeed("python /etc/attic-metrics-test.py corrupt")
    server.succeed("python /etc/attic-metrics-test.py cf-corrupt")
    attic.succeed("python /etc/attic-metrics-test.py restore")
    server.succeed("python /etc/attic-metrics-test.py cf-verify '" + json.dumps(expected) + "'")
    attic.succeed("python /etc/attic-metrics-test.py final-attic")
    server.succeed("python /etc/attic-metrics-test.py final-cf")
    attic.succeed("python /etc/attic-metrics-test.py module-lifecycle")
  '';
}
