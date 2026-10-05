{
  lib,
  inputs,
  system ? null,
  ...
}: rec {
  # These reproducible private keys are public test data, never production keys.
  # Certificates are issued at build time so isolated VMs use valid TLS dates.
  makeNiks3TestCredentials = {pkgs, extraDnsNames ? [], strictTls ? false}: pkgs.runCommand "niks3-test-credentials" {
    nativeBuildInputs = [pkgs.openssl (pkgs.python3.withPackages (p: [p.pynacl]))];
  } ''
    mkdir -p "$out"
    export OUT="$out"
    python - <<'PY'
    import base64, os
    from pathlib import Path
    from nacl.signing import SigningKey
    out = Path(os.environ["OUT"])
    for index, name in enumerate(["ca", "server", "write", "read", "wrong"]):
        seed = bytes([index + 1]) * 32
        der = bytes.fromhex("302e020100300506032b657004220420") + seed
        (out / f"{name}.key").write_text(
            "-----BEGIN PRIVATE KEY-----\n" + base64.b64encode(der).decode()
            + "\n-----END PRIVATE KEY-----\n")
    for index in range(2):
        key = SigningKey(bytes([index + 10]) * 32)
        name = f"niks3-test-{index}"
        (out / f"signing-{index}.key").write_text(
            name + ":" + base64.b64encode(bytes(key) + bytes(key.verify_key)).decode() + "\n")
        (out / f"signing-{index}.pub").write_text(
            name + ":" + base64.b64encode(bytes(key.verify_key)).decode() + "\n")
    PY
    openssl req -new -x509 -key "$out/ca.key" -out "$out/ca.crt" \
      -subj /CN=niks3-test-ca -days 3650 -set_serial 1 ${lib.optionalString strictTls "-addext 'keyUsage=critical,keyCertSign,cRLSign'"}
    for name in server write read wrong; do
      openssl req -new -key "$out/$name.key" -out "$name.csr" -subj "/CN=$name"
      if [ "$name" = server ]; then
        printf '%s\n' 'subjectAltName=DNS:cache,DNS:server,DNS:localhost,IP:127.0.0.1${lib.concatMapStrings (name: ",DNS:${name}") extraDnsNames}' 'extendedKeyUsage=serverAuth' > extensions
      else
        printf '%s\n' 'extendedKeyUsage=clientAuth' > extensions
      fi
      ${lib.optionalString strictTls ''
        # Python's current native TLS verifier requires RFC 5280 key usage and
        # issuer identity. Keep legacy certificate generation unchanged unless
        # the caller requests this stricter fixture profile.
        printf '%s\n' 'keyUsage=critical,digitalSignature' 'authorityKeyIdentifier=keyid,issuer' 'subjectKeyIdentifier=hash' >> extensions
      ''}
      openssl x509 -req -in "$name.csr" -CA "$out/ca.crt" -CAkey "$out/ca.key" \
        -set_serial "$(case "$name" in server) echo 2;; write) echo 3;; read) echo 4;; wrong) echo 5;; esac)" \
        -days 3650 -extfile extensions -out "$out/$name.crt"
    done
  '';

  # Owns all persistence inside one disposable NixOS VM. Garage's test-only
  # backend credentials remain on this node; clients receive only presigned
  # URLs from Niks3. Native TLS verifies separate read/write client subjects.
  makeNiks3CacheNode = {
    pkgs,
    credentials ? makeNiks3TestCredentials {inherit pkgs;},
    port ? 5751,
    # Discovery must advertise the exact configured read plane. Existing native
    # upload fixtures retain their public read URL unless callers opt in.
    cacheUrl ? "https://cache:${toString port}",
    tlsReadProxy ? true,
    enableFirewall ? false,
    ...
  }: {
    imports = [(makeS3CacheNode {
      inherit pkgs enableFirewall;
      accessKey = "GK0123456789abcdef01234567";
      importCredentials = true;
    })];
    # Garage's INFO access logs include presigned upload URLs. Keep those
    # bearer capabilities out of fixture journals and test-driver output.
    systemd.services.garage.environment.RUST_LOG = lib.mkForce "warn";
    virtualisation.useNixStoreImage = true;
    environment.systemPackages = [pkgs.crystal-forge.default.niks3 pkgs.curl pkgs.jq];
    services.postgresql = {
      enable = true;
      ensureDatabases = ["niks3"];
      ensureUsers = [{name = "niks3"; ensureDBOwnership = true;}];
    };
    users.users.niks3 = {isSystemUser = true; group = "niks3";};
    users.groups.niks3 = {};
    security.pki.certificateFiles = ["${credentials}/ca.crt"];
    networking.firewall.allowedTCPPorts = lib.mkIf enableFirewall [port 5752 5753];
    systemd.services.niks3-secrets = {
      after = ["garage-setup.service"];
      requires = ["garage-setup.service"];
      requiredBy = ["niks3.service"];
      before = ["niks3.service"];
      path = [pkgs.garage pkgs.coreutils];
      serviceConfig = {Type = "oneshot"; RemainAfterExit = true;};
      script = ''
        umask 077
        install -d -o niks3 -g niks3 /run/niks3
        printf '%s\n' GK0123456789abcdef01234567 > /run/niks3/access-key
        printf '%s\n' 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef > /run/niks3/secret-key
        printf '%s\n' 'niks3-nonproduction-static-token-470-00000000' > /run/niks3/token
        cp ${credentials}/signing-*.key /run/niks3/
        cp ${credentials}/server.key /run/niks3/server.key
        chown -R niks3:niks3 /run/niks3
      '';
    };
    systemd.services.niks3 = {
      wantedBy = ["multi-user.target"];
      after = ["postgresql.service" "niks3-secrets.service"];
      requires = ["postgresql.service" "niks3-secrets.service"];
      serviceConfig = {
        User = "niks3";
        Group = "niks3";
        ExecStart = lib.concatStringsSep " " [
          "${pkgs.crystal-forge.default.niks3}/bin/niks3-server"
          "--db 'dbname=niks3 user=niks3 host=/run/postgresql'"
          "--http-addr 0.0.0.0:${toString port}"
          "--s3-endpoint cache:3900 --s3-use-ssl=false --s3-region garage"
          "--s3-bucket nix-cache"
          "--s3-access-key-path /run/niks3/access-key"
          "--s3-secret-key-path /run/niks3/secret-key"
          "--api-token-path /run/niks3/token"
          "--sign-key-path /run/niks3/signing-0.key --sign-key-path /run/niks3/signing-1.key"
          "--tls-cert ${credentials}/server.crt --tls-key /run/niks3/server.key"
          "--tls-client-ca ${credentials}/ca.crt --mtls-bound-subject CN=write"
          "--enable-read-proxy --cache-url ${lib.escapeShellArg cacheUrl}"
        ];
      };
    };
    # Gate a distinct read URL with the read identity. Do not forward a caller's
    # certificate-verification header to Niks3. The write API is unreachable
    # through this read-only proxy, even with a valid read certificate.
    services.nginx = lib.mkIf tlsReadProxy {
      enable = true;
      virtualHosts.private-read = {
        addSSL = true;
        listen = [{addr = "0.0.0.0"; port = 5752; ssl = true;}];
        sslCertificate = "${credentials}/server.crt";
        sslCertificateKey = "${credentials}/server.key";
        extraConfig = ''
          ssl_client_certificate ${credentials}/ca.crt;
          ssl_verify_client on;
          if ($ssl_client_s_dn != "CN=read") { return 403; }
        '';
        locations."/api/".return = "403";
        locations."/" = {
          proxyPass = "https://127.0.0.1:${toString port}";
          extraConfig = ''
            proxy_ssl_verify on;
            proxy_ssl_trusted_certificate ${credentials}/ca.crt;
            proxy_ssl_name localhost;
            proxy_set_header X-SSL-Client-Verify "";
          '';
        };
      };
      virtualHosts.public-read = {
        addSSL = true;
        listen = [{addr = "0.0.0.0"; port = 5753; ssl = true;}];
        sslCertificate = "${credentials}/server.crt";
        sslCertificateKey = "${credentials}/server.key";
        locations."/api/".return = "403";
        locations."/" = {
          proxyPass = "https://127.0.0.1:${toString port}";
          extraConfig = ''
            proxy_ssl_verify on;
            proxy_ssl_trusted_certificate ${credentials}/ca.crt;
            proxy_ssl_name localhost;
          '';
        };
      };
    };
  };

  makeS3CacheNode = {
    pkgs,
    bucketName ? "nix-cache",
    accessKey ? "GK1234567890123456789",
    secretKey ? "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    importCredentials ? false,
    port ? 3900,
    enableFirewall ? false,
    extraConfig ? {},
    ...
  }:
    {
      virtualisation.writableStore = true;
      virtualisation.memorySize = 2048;

      networking.useDHCP = true;
      networking.firewall.enable = enableFirewall;
      networking.firewall.allowedTCPPorts = lib.mkIf enableFirewall [port];

      # Garage S3-compatible storage
      services.garage = {
        enable = true;
        package = pkgs.garage;
        settings = {
          replication_mode = "none";
          rpc_bind_addr = "127.0.0.1:3901";
          rpc_public_addr = "127.0.0.1:3901";

          # Test-only deterministic 32-byte secret.
          # Garage requires exactly 64 hexadecimal characters.
          rpc_secret = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

          s3_api = {
            api_bind_addr = "0.0.0.0:${toString port}";
            s3_region = "garage";
          };
          s3_web = {
            bind_addr = "127.0.0.1:3902";
            root_domain = ".s3.garage.localhost";
          };
          admin = {
            api_bind_addr = "127.0.0.1:3903";
          };
        };
      };

      # Setup bucket and credentials
      systemd.services.garage-setup = {
        after = ["network-online.target" "garage.service"];
        wants = ["network-online.target" "garage.service"];
        requires = ["garage.service"];
        wantedBy = ["multi-user.target"];

        environment = {
          PATH = lib.mkForce "${pkgs.garage}/bin:${pkgs.coreutils}/bin:${pkgs.curl}/bin:${pkgs.gnugrep}/bin:${pkgs.gawk}/bin:${pkgs.jq}/bin";
        };

        script = ''
          set -euo pipefail
          echo "Starting Garage setup for bucket: ${bucketName}"

          # Wait for Garage to be ready
          for i in {1..60}; do
            if curl -fsS http://127.0.0.1:3903/health >/dev/null 2>&1; then
              echo "Garage admin API is ready after $i attempts"
              break
            fi
            if [ "$i" -eq 60 ]; then
              echo "ERROR: Garage failed to start after 60 attempts"
              exit 1
            fi
            echo "Waiting for Garage... attempt $i/60"
            sleep 2
          done

          # Get node ID (capture full first field, not just first 16 chars)
          NODE_ID="$(
            garage status 2>/dev/null |
              awk '/^[0-9a-f]+[[:space:]]/ { print $1; exit }'
          )"
          
          if [ -z "$NODE_ID" ]; then
            echo "ERROR: Could not determine Garage node ID"
            garage status || true
            exit 1
          fi
          
          echo "Garage node ID: $NODE_ID"

          # Configure node
          garage layout assign "$NODE_ID" -c 1G -z test-zone
          garage layout apply --version 1 2>/dev/null || echo "Layout already applied"

          # Create bucket
          garage bucket info "${bucketName}" >/dev/null 2>&1 || \
            garage bucket create "${bucketName}"

          # Create API key
          ${if importCredentials then ''
            garage key import --yes -n test-key '${accessKey}' '${secretKey}' >/dev/null
          '' else ''
            garage key info test-key >/dev/null 2>&1 || \
              garage key create test-key >/dev/null
          ''}

          # Allow key to access bucket
          garage bucket allow --read --write "${bucketName}" --key test-key

          echo "Garage setup completed successfully"
        '';

        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = true;
          User = "root";
          Group = "root";
        };
      };
    }
    // extraConfig;

  makeAtticCacheNode = {
    pkgs,
    lib,
    port ? 8080,
    enableFirewall ? false,
    extraConfig ? {},
    ...
  }: let
    # Find the Attic *client* package across nixpkgs variants
    atticClient =
      pkgs.attic or pkgs.attic-client or (throw ''
        Attic client package not found in pkgs.
        Tried: pkgs.attic and pkgs.attic-client.
        Fix by:
          • Updating nixpkgs to a revision that includes Attic, or
          • Adding an overlay/input that provides the Attic client.
      '');
  in
    {
      virtualisation.writableStore = true;
      virtualisation.memorySize = 1024;

      networking.useDHCP = true;
      networking.firewall.enable = enableFirewall;
      networking.firewall.allowedTCPPorts = lib.mkIf enableFirewall [port];

      # Server is usually pkgs.attic-server; client is detected above
      environment.systemPackages = [
        atticClient
        pkgs.attic-server
        pkgs.curl
        pkgs.coreutils
      ];

      users.users.attic = {
        description = "Attic service user";
        isSystemUser = true;
        group = "attic";
        home = "/var/lib/attic";
        createHome = true;
      };
      users.groups.attic = {};

      # PostgreSQL setup for Attic
      services.postgresql = {
        enable = true;
        ensureDatabases = ["attic"];
        ensureUsers = [
          {
            name = "attic";
            ensureDBOwnership = true;
          }
        ];
        authentication = ''
          local all all peer
          host all all 127.0.0.1/32 trust
          host all all ::1/128 trust
        '';
      };

      environment.etc."atticd.toml".text = ''
        listen = "0.0.0.0:${toString port}"

        [database]
        url = "postgresql://attic@localhost/attic"

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
        description = "Attic Cache Daemon";
        wantedBy = ["multi-user.target"];
        after = ["network-online.target" "postgresql.service"];
        wants = ["network-online.target"];
        requires = ["postgresql.service"];

        environment = {
          ATTICD_SERVER_TOKEN_HS256_SECRET_BASE64 = "dGVzdCBzZWNyZXQgZm9yIGF0dGljZA==";
        };

        serviceConfig = {
          ExecStart = "${pkgs.attic-server}/bin/atticd --config /etc/atticd.toml";
          Restart = "always";
          RestartSec = 10;
          User = "attic";
          Group = "attic";
          StateDirectory = "attic";
          StateDirectoryMode = "0755";
          WorkingDirectory = "/var/lib/attic";
          ReadWritePaths = "/var/lib/attic";
        };
      };

      systemd.services.attic-setup = {
        description = "Attic Cache Setup";
        after = ["atticd.service" "postgresql.service"];
        requires = ["atticd.service" "postgresql.service"];
        wantedBy = ["multi-user.target"];

        environment = {
          PATH = lib.mkForce "${pkgs.systemd}/bin:${pkgs.attic-server}/bin:${atticClient}/bin:${pkgs.curl}/bin:${pkgs.coreutils}/bin:${pkgs.gnugrep}/bin";
        };

        script = ''
          set -euo pipefail
          echo "Starting Attic cache setup..."

          BASE_URL="http://127.0.0.1:${toString port}"

          # Wait for API to be reachable (any 2xx/3xx/4xx means listener is up)
          for i in {1..60}; do
            if curl -sf -o /dev/null -w "%{http_code}" "$BASE_URL/" | grep -qE '^(2|3|4)'; then
              echo "atticd is up after $i attempts"
              break
            fi
            if [ "$i" -eq 60 ]; then
              echo "ERROR: atticd did not become ready"
              systemctl status atticd.service || true
              journalctl -u atticd.service --no-pager -n 100 || true
              exit 1
            fi
            sleep 2
          done

          # Mint a token with wide perms using the SAME config/secret as atticd
          TOKEN="$(${pkgs.attic-server}/bin/atticadm --config /etc/atticd.toml \
            make-token --sub setup --validity 1d \
            --pull '*' --push '*' --create-cache '*' --configure-cache '*')"

          # Login alias "local"
          ${atticClient}/bin/attic login local "$BASE_URL" "$TOKEN"

          # Create cache "test" if missing (idempotent)
          if ! ${atticClient}/bin/attic cache info local:test >/dev/null 2>&1; then
            ${atticClient}/bin/attic cache create local:test
          fi

          # Make it public (idempotent)
          ${atticClient}/bin/attic cache configure local:test --public || true

          echo "Attic setup completed successfully"
        '';

        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = true;
          User = "root";
          Group = "root";
        };
      };

      systemd.services.attic-debug = {
        description = "Attic Debug Info";
        after = ["attic-setup.service"];
        wantedBy = ["multi-user.target"];

        path = with pkgs; [
          iproute2 # provides `ss`
          curl
          coreutils
          systemd # provides `systemctl`
        ];
        # in makeAtticCacheNode -> systemd.services.attic-debug.script
        script = ''
          echo "=== Attic Debug Info ==="
          ss -tlnp | grep ":${toString port}" || echo "Nothing listening on ${toString port}"
          curl -sv "http://127.0.0.1:${toString port}/" || true

          TOKEN="$(${pkgs.attic-server}/bin/atticadm --config /etc/atticd.toml \
              make-token --sub debug --validity 5m \
              --pull '*' --push '*' --create-cache '*' --configure-cache '*')"

          ${atticClient}/bin/attic login debug "http://127.0.0.1:${toString port}" "$TOKEN" || true
          ${atticClient}/bin/attic cache info debug:test || true

          systemctl status atticd.service || true
          ls -la /var/lib/attic/ || true
          echo "=== End Debug Info ==="
        '';

        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = true;
        };
      };
    }
    // extraConfig;
}
