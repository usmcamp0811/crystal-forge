{
  lib,
  inputs,
  system ? null,
  ...
}: rec {
  # These reproducible private keys are public test data, never production keys.
  # Certificates are issued at build time so isolated VMs use valid TLS dates.
  makeNiks3TestCredentials = {pkgs, extraDnsNames ? [], strictTls ? false, productionPki ? false}: let
    generated = pkgs.runCommand "niks3-test-credentials" {
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
    ${lib.optionalString productionPki ''
      mkdir -p "$out/production"
      python - <<'PY'
      import base64, os
      from pathlib import Path
      from nacl.signing import SigningKey
      out = Path(os.environ['OUT']) / 'production'
      names = ['server-ca-a','server-ca-b','client-ca-c','push-server','s3-server',
               'write-client-1','write-client-2','wrong-subject','wrong-issuer','read-server']
      for index, name in enumerate(names):
          der = bytes.fromhex('302e020100300506032b657004220420') + bytes([index + 40]) * 32
          (out / (name + '.key')).write_text('-----BEGIN PRIVATE KEY-----\n' + base64.b64encode(der).decode() + '\n-----END PRIVATE KEY-----\n')
      for index in range(2):
          key = SigningKey(bytes([index + 60]) * 32)
          name = 'niks3-production-' + str(index)
          (out / ('signing-' + str(index) + '.key')).write_text(name + ':' + base64.b64encode(bytes(key) + bytes(key.verify_key)).decode() + '\n')
          (out / ('signing-' + str(index) + '.pub')).write_text(name + ':' + base64.b64encode(bytes(key.verify_key)).decode() + '\n')
      PY
      for ca in server-ca-a server-ca-b client-ca-c; do
        openssl req -new -x509 -key "$out/production/$ca.key" -out "$out/production/$ca.crt" \
          -subj "/CN=niks3-$ca" -days 3650 -set_serial 70 \
          -addext 'basicConstraints=critical,CA:TRUE' \
          -addext 'keyUsage=critical,keyCertSign,cRLSign' -addext 'subjectKeyIdentifier=hash'
      done
      for name in push-server s3-server write-client-1 write-client-2 wrong-subject wrong-issuer read-server; do
        case "$name" in
          push-server) issuer="$out/production/server-ca-a"; subject=push-cache.test; usage=serverAuth; san='DNS:push-cache.test,DNS:cache,DNS:localhost,IP:127.0.0.1';;
          s3-server) issuer="$out/production/server-ca-b"; subject=s3-cache.test; usage=serverAuth; san='DNS:s3-cache.test';;
          read-server) issuer="$out/production/server-ca-b"; subject=read-cache.test; usage=serverAuth; san='DNS:read-cache.test,DNS:read-foreign.test';;
          wrong-issuer) issuer="$out/production/server-ca-b"; subject=write; usage=clientAuth; san="";;
          wrong-subject) issuer="$out/production/client-ca-c"; subject=wrong; usage=clientAuth; san="";;
          *) issuer="$out/production/client-ca-c"; subject=write; usage=clientAuth; san="";;
        esac
        openssl req -new -key "$out/production/$name.key" -out "$name.csr" -subj "/CN=$subject"
        printf '%s\n' "extendedKeyUsage=$usage" 'basicConstraints=critical,CA:FALSE' \
          'keyUsage=critical,digitalSignature' 'subjectKeyIdentifier=hash' 'authorityKeyIdentifier=keyid,issuer' > extensions
        if test -n "$san"; then printf '%s\n' "subjectAltName=$san" >> extensions; fi
        openssl x509 -req -in "$name.csr" -CA "$issuer.crt" -CAkey "$issuer.key" \
          -set_serial "$(case "$name" in push-server) echo 81;; s3-server) echo 82;; write-client-1) echo 83;; write-client-2) echo 84;; wrong-subject) echo 85;; wrong-issuer) echo 86;; read-server) echo 87;; esac)" \
          -days 3650 -extfile extensions -out "$out/production/$name.crt"
      done
      cat "$out/production/server-ca-a.crt" "$out/production/server-ca-b.crt" > "$out/production/server-roots-ab.pem"
      # Public synthetic fixture values. Consumers pass protected file paths,
      # never username/password arguments, to native read commands.
      printf '%s' 'cf-basic-read-fixture-470' > "$out/production/basic-username"
      printf '%s' 'cf-basic-read-password-fixture-470' > "$out/production/basic-password"
      printf '%s:' 'cf-basic-read-fixture-470' > "$out/production/basic.htpasswd"
      openssl passwd -apr1 -stdin < "$out/production/basic-password" >> "$out/production/basic.htpasswd"
    ''}
  '';
  in generated // lib.optionalAttrs productionPki {
    # Shared fixture contract. CA roots verify SERVERS; clientCA verifies client
    # identities at nginx. No production user material is stored in these paths.
    productionWritePki = let
      aliases = pkgs.runCommand "niks3-production-write-pki" {} ''
        mkdir -p "$out"
        cp ${generated}/production/* "$out/"
        ln -s server-ca-a.crt "$out/api-ca.crt"
        ln -s server-ca-b.crt "$out/s3-ca.crt"
        ln -s client-ca-c.crt "$out/client-ca.crt"
        ln -s push-server.crt "$out/api-server.crt"
        ln -s push-server.key "$out/api-server.key"
        ln -s write-client-1.crt "$out/write-client.crt"
        ln -s write-client-1.key "$out/write-client.key"
        ln -s write-client-2.crt "$out/replacement-write-client.crt"
        ln -s write-client-2.key "$out/replacement-write-client.key"
        ln -s server-roots-ab.pem "$out/server-ca-bundle.pem"
      '';
      certificate = name: {cert = "${generated}/production/${name}.crt"; key = "${generated}/production/${name}.key";};
    in aliases // {
      roots = "${generated}/production/server-roots-ab.pem";
      serverA = "${generated}/production/server-ca-a.crt";
      serverB = "${generated}/production/server-ca-b.crt";
      clientCA = "${generated}/production/client-ca-c.crt";
      writeClient = certificate "write-client-1";
      replacementClient = certificate "write-client-2";
      wrongSubject = certificate "wrong-subject";
      wrongIssuer = certificate "wrong-issuer";
      apiServer = certificate "push-server";
      s3Server = certificate "s3-server";
      readServer = certificate "read-server";
      signingPublicKeys = map (index: "${generated}/production/signing-${toString index}.pub") [0 1];
      signingPrivateKeys = map (index: "${generated}/production/signing-${toString index}.key") [0 1];
    };
  };

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
    productionPki ? false,
    basicRead ? false,
    ...
  }: if productionPki then makeNiks3ProductionCacheNode {inherit pkgs credentials enableFirewall basicRead;} else {
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

  # Opt-in production-shaped transport. Niks3 1.6.0 has no UNIX listener. The
  # private socket bridge is a fixture adaptation, not a Crystal Forge dependency.
  makeNiks3ProductionCacheNode = {pkgs, credentials, enableFirewall ? false, basicRead ? false}: let
    pki = credentials.productionWritePki;
    nativePort = 5755;
    bridgeUid = 63071;
    socket = "/run/niks3-production-api/native.sock";
  in {
    imports = [(makeS3CacheNode {
      inherit pkgs enableFirewall;
      bucketName = "production-niks3";
      accessKey = "GK0123456789abcdef01234567";
      importCredentials = true;
    })];
    virtualisation.useNixStoreImage = true;
    environment.systemPackages = [pkgs.crystal-forge.default.niks3 pkgs.python3 pkgs.curl pkgs.jq];
    security.pki.certificateFiles = [pki.serverB];
    environment.etc."niks3-production-pki".source = "${credentials}/production";
    services.garage.settings = {
      rpc_bind_addr = lib.mkForce "127.0.0.1:3911";
      rpc_public_addr = lib.mkForce "127.0.0.1:3911";
      s3_api.api_bind_addr = lib.mkForce "127.0.0.1:3900";
    };
    # Access/query strings can carry presigned capabilities. Only the separate
    # nginx boolean observer is an audit source; native Garage logs stay quiet.
    systemd.services.garage.environment.RUST_LOG = lib.mkForce "error";
    services.postgresql = {
      enable = true;
      ensureDatabases = ["niks3-production"];
      ensureUsers = [{name = "niks3-production"; ensureDBOwnership = true;}];
    };
    users.groups = {niks3-production = {}; niks3-proxy = {};};
    users.users = {
      niks3-production = {isSystemUser = true; group = "niks3-production"; uid = 63070;};
      niks3-bridge = {isSystemUser = true; group = "niks3-proxy"; uid = bridgeUid;};
      nginx.extraGroups = ["niks3-proxy"];
    };
    # SECURITY: No UID except the socket bridge can connect to the native TCP
    # origin, including root test clients. Native binds IPv4 loopback only. The
    # UNIX socket is the only nginx ingress to the trusted-header auth channel.
    networking.nftables = {
      enable = true;
      tables.niks3-origin = {
        family = "inet";
        content = ''
          chain output {
            type filter hook output priority -10; policy accept;
            ip daddr 127.0.0.1 tcp dport ${toString nativePort} meta skuid != ${toString bridgeUid} reject with tcp reset
          }
        '';
      };
    };
    networking.firewall.allowedTCPPorts = lib.mkIf enableFirewall [5754 5753 3901];
    systemd.services.niks3-production-secrets = {
      before = ["niks3-production.service"];
      requiredBy = ["niks3-production.service"];
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        User = "niks3-production";
        Group = "niks3-production";
        RuntimeDirectory = "niks3-production-secrets";
        RuntimeDirectoryMode = "0700";
      };
      script = ''
        umask 077
        printf '%s' GK0123456789abcdef01234567 > /run/niks3-production-secrets/access-key
        printf '%s' 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef > /run/niks3-production-secrets/secret-key
        printf '%s' private-server-only-api-token-production-470 > /run/niks3-production-secrets/api-token
        cp ${lib.escapeShellArg (builtins.elemAt pki.signingPrivateKeys 0)} /run/niks3-production-secrets/signing-0.key
        cp ${lib.escapeShellArg (builtins.elemAt pki.signingPrivateKeys 1)} /run/niks3-production-secrets/signing-1.key
      '';
    };
    systemd.services.niks3-production = {
      wantedBy = ["multi-user.target"];
      after = ["postgresql.service" "garage-setup.service" "nginx.service" "niks3-production-secrets.service" "nftables.service"];
      requires = ["postgresql.service" "garage-setup.service" "nginx.service" "niks3-production-secrets.service" "nftables.service"];
      serviceConfig = {
        User = "niks3-production";
        Group = "niks3-production";
        ExecStart = lib.concatStringsSep " " [
          "${pkgs.crystal-forge.default.niks3}/bin/niks3-server"
          "--db 'dbname=niks3-production user=niks3-production host=/run/postgresql'"
          "--http-addr 127.0.0.1:${toString nativePort}"
          "--s3-endpoint s3-cache.test:3901 --s3-use-ssl=true --s3-region garage"
          "--s3-bucket production-niks3"
          "--s3-access-key-path /run/niks3-production-secrets/access-key"
          "--s3-secret-key-path /run/niks3-production-secrets/secret-key"
          "--api-token-path /run/niks3-production-secrets/api-token"
          "--sign-key-path /run/niks3-production-secrets/signing-0.key --sign-key-path /run/niks3-production-secrets/signing-1.key"
          "--mtls-proxy-header X-SSL-Client-Verify --mtls-subject-header X-SSL-Client-Dn --mtls-bound-subject CN=write"
          "--enable-read-proxy --cache-url https://read-cache.test:5753"
        ];
      };
    };
    systemd.services.niks3-production-bridge = {
      wantedBy = ["multi-user.target"];
      after = ["niks3-production.service" "nftables.service"];
      requires = ["niks3-production.service" "nftables.service"];
      serviceConfig = {
        User = "niks3-bridge";
        Group = "niks3-proxy";
        RuntimeDirectory = "niks3-production-api";
        RuntimeDirectoryMode = "0750";
        UMask = "0007";
        ExecStart = "${pkgs.socat}/bin/socat UNIX-LISTEN:${socket},fork,mode=0660 TCP:127.0.0.1:${toString nativePort}";
        NoNewPrivileges = true;
      };
    };
    services.nginx = {
      enable = true;
      clientMaxBodySize = "128m";
      commonHttpConfig = ''
        map $http_authorization $production_authorization { "" false; default true; }
        map $ssl_client_verify $production_certificate { SUCCESS true; default false; }
        map $ssl_client_s_dn $production_write_subject { "CN=write" true; default false; }
        map $request_method $production_put { PUT true; default false; }
        map $request_method $production_post { POST true; default false; }
        map $remote_addr $production_backend { 127.0.0.1 true; default false; }
        map $status $production_signature { ~^2 true; default false; }
        log_format production_observer escape=json '{"authorization":$production_authorization,"client_certificate":$production_certificate,"write_subject":$production_write_subject,"put":$production_put,"post":$production_post,"backend":$production_backend,"signature_validated":$production_signature,"status":$status}';
      '';
      virtualHosts = {
        production-api = {
          addSSL = true;
          listen = [{addr = "0.0.0.0"; port = 5754; ssl = true;}];
          serverName = "push-cache.test";
          sslCertificate = pki.apiServer.cert;
          sslCertificateKey = pki.apiServer.key;
          extraConfig = ''
            ssl_client_certificate ${pki.clientCA};
            ssl_verify_client on;
            access_log /var/log/nginx/production-api-observer.json production_observer;
            error_log /dev/null;
          '';
          locations."/" = {
            proxyPass = "http://unix:${socket}:";
            extraConfig = ''
              proxy_set_header X-SSL-Client-Verify $ssl_client_verify;
              proxy_set_header X-SSL-Client-Dn $ssl_client_s_dn;
              proxy_set_header Authorization "";
            '';
          };
        };
        production-s3 = {
          addSSL = true;
          listen = [{addr = "0.0.0.0"; port = 3901; ssl = true;}];
          serverName = "s3-cache.test";
          sslCertificate = pki.s3Server.cert;
          sslCertificateKey = pki.s3Server.key;
          extraConfig = ''
            access_log /var/log/nginx/production-s3-observer.json production_observer;
            error_log /dev/null;
            if ($http_host != "s3-cache.test:3901") { return 400; }
          '';
          locations."/" = {
            proxyPass = "http://127.0.0.1:3900";
            # Preserve the exact configured signed authority, including port.
            # Reject any different inbound Host rather than proxying a spoof.
            extraConfig = ''proxy_set_header Host "s3-cache.test:3901";'';
          };
        };
        production-read = {
          addSSL = true;
          listen = [{addr = "0.0.0.0"; port = 5753; ssl = true;}];
          serverName = "read-cache.test";
          sslCertificate = pki.readServer.cert;
          sslCertificateKey = pki.readServer.key;
          extraConfig = ''
            access_log /var/log/nginx/production-read-observer.json production_observer;
            error_log /dev/null;
            ${lib.optionalString basicRead ''
              auth_basic "Niks3 read";
              auth_basic_user_file ${credentials}/production/basic.htpasswd;
              error_page 401 =403 /basic-denied;
            ''}
          '';
          locations = {
            "/api/".return = "403";
            "/" = {
              proxyPass = "http://unix:${socket}:";
              extraConfig = ''
                proxy_set_header X-SSL-Client-Verify "";
                proxy_set_header X-SSL-Client-Dn "";
                proxy_set_header Authorization "";
              '';
            };
          # Each guarded transfer must stop at the source response. Targets
          # have a separate boolean-only observer so zero requests is provable.
          } // lib.optionalAttrs basicRead (lib.listToAttrs (map (case: {
            name = "/guard-${case.name}/";
            value.return = "${toString case.status} ${case.target}";
          }) [
            {name = "same-path"; status = 301; target = "https://read-cache.test:5753/guard-target/";}
            {name = "hostname"; status = 302; target = "https://read-foreign.test:5756/guard-target/";}
            {name = "port"; status = 303; target = "https://read-cache.test:5756/guard-target/";}
            {name = "downgrade"; status = 307; target = "http://read-cache.test:5757/guard-target/";}
            {name = "permanent"; status = 308; target = "https://read-cache.test:5753/guard-target/";}
            {name = "not-modified"; status = 304; target = "https://read-cache.test:5753/guard-target/";}
          ]) // {
            "/basic-denied" = {
              extraConfig = ''internal; auth_basic off;'';
              return = "403";
            };
            "/guard-target/" = {
              extraConfig = ''auth_basic off; access_log /var/log/nginx/production-target-observer.json production_observer;'';
              return = "200 'StoreDir: /nix/store\nWantMassQuery: 1\nPriority: 40\n'";
            };
            "/guard-nar-host/".alias = "/run/niks3-basic-guard/host/";
            "/guard-nar-port/".alias = "/run/niks3-basic-guard/port/";
          });
        };
        production-read-target = lib.mkIf basicRead {
          addSSL = true;
          listen = [{addr = "0.0.0.0"; port = 5756; ssl = true;} {addr = "0.0.0.0"; port = 5757; ssl = false;}];
          serverName = "read-foreign.test";
          sslCertificate = pki.readServer.cert;
          sslCertificateKey = pki.readServer.key;
          extraConfig = ''access_log /var/log/nginx/production-target-observer.json production_observer; error_log /dev/null;'';
          locations."/".return = "200 'StoreDir: /nix/store\nWantMassQuery: 1\nPriority: 40\n'";
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
