{inputs, lib, pkgs, ...}: let
  # Exact declaration from checks/web-ui/default.nix. Production remains 1.6.
  statsNiks3 = pkgs.buildGoModule {
    pname = "web-ui-native-niks3-stats-server";
    version = "1.8.0";
    src = pkgs.fetchFromGitHub {
      owner = "Mic92"; repo = "niks3"; rev = "61501c4223ec3ad804c20625128cab1155166041";
      hash = "sha256-86afR/fMjLRLmBEMFA6ow6SRI9T5Qe83l0em/FciW2g=";
    };
    vendorHash = "sha256-qkB99S/9fmSk5G9uHyQF/z+joi9JACIJWaHMrIo4ziU=";
    subPackages = ["cmd/niks3-server"];
    ldflags = ["-s"];
  };
  python = pkgs.python3.withPackages (p: [p.requests p.psycopg2 p.cryptography p.pynacl p.boto3]);
  script = ./test.py;
  native = package: database: port: readPort: {
    wantedBy = ["multi-user.target"];
    after = ["postgresql.service" "garage-setup.service" "probe-secrets.service" "nginx.service"];
    requires = ["postgresql.service" "garage-setup.service" "probe-secrets.service" "nginx.service"];
    environment.SSL_CERT_FILE = "/run/niks3-probe/roots-ab.pem";
    serviceConfig = {
      User = "niks3";
      Group = "niks3";
      ExecStart = lib.concatStringsSep " " [
        "${package}/bin/niks3-server"
        "--db 'dbname=${database} user=niks3 host=/run/postgresql'"
        "--http-addr 0.0.0.0:${toString port}"
        "--s3-endpoint cache:3904 --s3-use-ssl=true --s3-region garage --s3-bucket nix-cache"
        "--s3-access-key-path /run/niks3-probe/access-key --s3-secret-key-path /run/niks3-probe/secret-key"
        "--api-token-path /run/niks3-probe/token"
        "--sign-key-path /run/niks3-probe/signing.key"
        "--tls-cert /run/niks3-probe/api.crt --tls-key /run/niks3-probe/api.key"
        # Deployed native 1.6 trust shape: union roots, separate bound subject.
        "--tls-client-ca /run/niks3-probe/roots-abc.pem --mtls-bound-subject CN=write"
        "--enable-read-proxy --cache-url https://cache:${toString readPort}/public/"
      ];
    };
  };
in assert pkgs.crystal-forge.default.niks3.version == "1.6.0"; pkgs.testers.runNixOSTest {
  name = "niks3-write-probe";
  skipLint = true;
  skipTypeCheck = true;
  globalTimeout = 1200;
  nodes = {
    cache = {
      # Reuse Garage provisioning unchanged; this check owns its native API units.
      imports = [(lib.crystal-forge.makeS3CacheNode {
        inherit pkgs;
        accessKey = "GK0123456789abcdef01234567";
        importCredentials = true;
      })];
      virtualisation = {useNixStoreImage = true; writableStore = true; memorySize = 3072; cores = 2;};
      environment.systemPackages = [python pkgs.busybox pkgs.crystal-forge.default.niks3 pkgs.nix-eval-jobs.nix];
      environment.etc."niks3-write-probe.py".source = script;
      networking.hosts."127.0.0.1" = ["cache"];
      nix.settings = {experimental-features = ["nix-command" "flakes"]; substituters = lib.mkForce [];};
      # Native invalid-token previews are an upstream limitation. Do not export
      # provider journals; do not weaken AuthMiddleware or its logging level.
      systemd.services.garage.environment.RUST_LOG = lib.mkForce "error";
      users.users.niks3 = {isSystemUser = true; group = "niks3";};
      users.groups.niks3 = {};
      services.postgresql = {
        enable = true;
        ensureDatabases = ["niks3" "niks3-v18"];
        ensureUsers = [{name = "niks3"; ensureDBOwnership = true;}];
        authentication = lib.mkForce "local all all trust";
      };
      systemd.services.probe-secrets = {
        before = ["nginx.service" "niks3.service" "niks3-v18.service" "probe-observer.service"];
        requiredBy = ["nginx.service" "niks3.service" "niks3-v18.service" "probe-observer.service"];
        after = ["postgresql.service"];
        requires = ["postgresql.service"];
        serviceConfig = {Type = "oneshot"; RemainAfterExit = true; UMask = "0077";};
        script = ''
          ${python}/bin/python /etc/niks3-write-probe.py prepare
          chown -R niks3:niks3 /run/niks3-probe
          ${pkgs.postgresql}/bin/psql -U postgres -d postgres -v ON_ERROR_STOP=1 \
            -c 'ALTER DATABASE "niks3-v18" OWNER TO niks3' >/dev/null
        '';
      };
      systemd.services.niks3 = native pkgs.crystal-forge.default.niks3 "niks3" 5751 9746;
      systemd.services.niks3-v18 = native statsNiks3 "niks3-v18" 5758 9748;
      systemd.services.probe-observer = {
        wantedBy = ["multi-user.target"];
        after = ["niks3.service" "niks3-v18.service"];
        requires = ["niks3.service" "niks3-v18.service"];
        serviceConfig = {
          ExecStart = "${python}/bin/python /etc/niks3-write-probe.py serve";
          UMask = "0077";
          StandardOutput = "null";
          StandardError = "null";
        };
      };
      services.nginx = {
        enable = true;
        virtualHosts.s3 = {
          addSSL = true;
          listen = [{addr = "0.0.0.0"; port = 3904; ssl = true;}];
          sslCertificate = "/run/niks3-probe/s3.crt";
          sslCertificateKey = "/run/niks3-probe/s3.key";
          extraConfig = "access_log off; error_log /dev/null;";
          locations."/" = {
            proxyPass = "http://127.0.0.1:3900";
            extraConfig = "proxy_set_header Host $http_host;";
          };
        };
      };
    };
    server = {lib, ...}: {
      imports = [inputs.self.nixosModules.crystal-forge];
      networking.firewall.enable = false;
      virtualisation = {useNixStoreImage = true; writableStore = true; memorySize = 4096; cores = 2;};
      environment.systemPackages = [python];
      environment.etc."niks3-write-probe.py".source = script;
      services.crystal-forge = {
        enable = true;
        client.enable = false;
        build.enable = false;
        hardening.enable = false;
        local-database = true;
        database = {host = "127.0.0.1"; user = "crystal_forge"; name = "crystal_forge";};
        server = {
          enable = true;
          package = pkgs.crystal-forge.default.cf-server-core-drv;
          host = "0.0.0.0";
          port = 8000;
          auth_mode = "local";
          allow_private_cache_test_targets = true;
        };
        cache = {push_after_build = false; encryption_key_file = "/run/niks3-probe/encryption-key";};
        flakes.watched = [];
        environments = [{name = "write-probe"; description = "Disposable native probe scope"; is_active = true; risk_profile = "LOW"; compliance_level = "NONE";}];
      };
      systemd.services.crystal-forge-server = {
        wantedBy = lib.mkForce [];
        environment = {AUTH_MODE = "local"; SSL_CERT_FILE = "/run/niks3-probe/roots-ab.pem";};
      };
      services.postgresql.authentication = lib.mkForce ''
        local all all trust
        host all all 127.0.0.1/32 trust
      '';
      systemd.services.crystal-forge-postgres-jobs.wantedBy = lib.mkForce [];
      systemd.timers.crystal-forge-postgres-jobs.wantedBy = lib.mkForce [];
      # The inspector normally Wants the server and would start it before its
      # runtime trust/encryption files arrive. This fixture has no evaluator work.
      systemd.services.crystal-forge-config-inspector.wantedBy = lib.mkForce [];
    };
  };
  testScript = ''
    import os
    import tempfile
    start_all()
    cache.wait_for_unit("niks3.service")
    cache.wait_for_unit("niks3-v18.service")
    cache.wait_for_unit("probe-observer.service")
    cache.wait_for_open_port(9080)
    # Sensitive material crosses only private files, never driver command text.
    with tempfile.TemporaryDirectory(prefix="niks3-write-probe-") as private:
        os.chmod(private, 0o700)
        cache.copy_from_vm("/run/niks3-probe/client.json", private)
        os.chmod(os.path.join(private, "client.json"), 0o600)
        server.copy_from_host(os.path.join(private, "client.json"), "/run/probe-client.json")
    server.succeed("chmod 0600 /run/probe-client.json; python /etc/niks3-write-probe.py install-client")
    server.succeed("systemctl start crystal-forge-server.service")
    server.wait_for_open_port(8000)
    cache.succeed("python /etc/niks3-write-probe.py native-contract")
    cache.succeed("python /etc/niks3-write-probe.py baseline")
    server.succeed("python /etc/niks3-write-probe.py matrix")
    cache.succeed("python /etc/niks3-write-probe.py unchanged")
    # This is fixture publication AFTER the non-mutating matrix, never Test.
    cache.succeed("python /etc/niks3-write-probe.py publication")
  '';
}
