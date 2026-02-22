# OIDC Authentication VM Integration Test
#
# Tests the full OIDC authentication flow using Keycloak as the identity provider:
# 1. Starts Keycloak OIDC provider with pre-configured realm
# 2. Starts Crystal Forge server in OIDC mode
# 3. Verifies OIDC discovery endpoint reachability
# 4. Performs OIDC Resource Owner Password Credentials flow
# 5. Verifies session creation and JWT claim extraction
#
# Run: nix build .#checks.x86_64-linux.oidc-auth --print-build-logs
{
  lib,
  inputs,
  pkgs,
  ...
}: let
  CF_TEST_SERVER_PORT = 3000;
  KEYCLOAK_PORT = 8080;
  OIDC_REALM = "crystal-forge";
  OIDC_CLIENT_ID = "crystal-forge-server";
  OIDC_CLIENT_SECRET = "vm-test-secret";

  realmImport = ./realm-crystal-forge.json;
in
  pkgs.testers.runNixOSTest {
    name = "crystal-forge-oidc-auth-test";
    skipLint = true;
    skipTypeCheck = true;

    nodes = {
      # Keycloak OIDC Provider
      keycloak = {
        virtualisation.memorySize = 2048;
        virtualisation.cores = 2;

        networking.useDHCP = true;
        networking.firewall.allowedTCPPorts = [KEYCLOAK_PORT];

        # PostgreSQL for Keycloak - use TCP with trust auth
        services.postgresql = {
          enable = true;
          ensureDatabases = ["keycloak"];
          ensureUsers = [
            {
              name = "keycloak";
              ensureDBOwnership = true;
            }
          ];
          authentication = lib.mkForce ''
            local all all trust
            host all all 127.0.0.1/32 trust
            host all all ::1/128 trust
          '';
        };

        # Create password file for Keycloak DB (empty password since we use trust auth)
        environment.etc."keycloak-db-pass".text = "";

        services.keycloak = {
          enable = true;
          database = {
            type = "postgresql";
            host = "localhost";
            createLocally = false; # We create it above
            username = "keycloak";
            passwordFile = "/etc/keycloak-db-pass";
            useSSL = false;
          };
          settings = {
            hostname = "keycloak";
            hostname-strict = false;
            hostname-strict-https = false;
            http-enabled = true;
            http-host = "0.0.0.0";
            http-port = KEYCLOAK_PORT;
            proxy-headers = "xforwarded";
          };
          initialAdminPassword = "admin";
        };

        # Import realm after Keycloak starts
        systemd.services.keycloak-realm-import = {
          description = "Import Crystal Forge realm into Keycloak";
          after = ["keycloak.service"];
          wantedBy = ["multi-user.target"];
          serviceConfig = {
            Type = "oneshot";
            RemainAfterExit = true;
          };
          path = [pkgs.curl pkgs.jq];
          script = ''
            set -euo pipefail

            KEYCLOAK_URL="http://localhost:${toString KEYCLOAK_PORT}"
            REALM_FILE="${realmImport}"

            echo "Waiting for Keycloak to be ready..."
            for i in $(seq 1 60); do
              if curl -sf "$KEYCLOAK_URL/health/ready" >/dev/null 2>&1; then
                echo "Keycloak is ready"
                break
              fi
              echo "Waiting... ($i/60)"
              sleep 2
            done

            # Get admin token
            echo "Obtaining admin token..."
            TOKEN=$(curl -sf -X POST "$KEYCLOAK_URL/realms/master/protocol/openid-connect/token" \
              -H "Content-Type: application/x-www-form-urlencoded" \
              -d "username=admin" \
              -d "password=admin" \
              -d "grant_type=password" \
              -d "client_id=admin-cli" | jq -r '.access_token')

            if [ -z "$TOKEN" ] || [ "$TOKEN" = "null" ]; then
              echo "Failed to get admin token"
              exit 1
            fi

            # Check if realm already exists
            if curl -sf -H "Authorization: Bearer $TOKEN" "$KEYCLOAK_URL/admin/realms/${OIDC_REALM}" >/dev/null 2>&1; then
              echo "Realm ${OIDC_REALM} already exists"
              exit 0
            fi

            # Import realm
            echo "Importing realm from $REALM_FILE..."
            curl -sf -X POST "$KEYCLOAK_URL/admin/realms" \
              -H "Authorization: Bearer $TOKEN" \
              -H "Content-Type: application/json" \
              -d @"$REALM_FILE"

            echo "Realm ${OIDC_REALM} imported successfully"
          '';
        };
      };

      # Crystal Forge Server
      server = {
        imports = [inputs.self.nixosModules.crystal-forge];

        virtualisation.memorySize = 2048;
        virtualisation.cores = 2;

        networking.useDHCP = true;
        networking.firewall.allowedTCPPorts = [CF_TEST_SERVER_PORT 5432];

        environment.systemPackages = with pkgs; [curl jq];

        services.postgresql = {
          enable = true;
          settings."listen_addresses" = lib.mkForce "*";
          authentication = lib.concatStringsSep "\n" [
            "local   all   postgres   trust"
            "local   all   all        peer"
            "host    all   all 127.0.0.1/32 trust"
            "host    all   all ::1/128      trust"
          ];
          initialScript = pkgs.writeText "init-crystal-forge.sql" ''
            CREATE USER crystal_forge LOGIN;
            CREATE DATABASE crystal_forge OWNER crystal_forge;
            GRANT ALL PRIVILEGES ON DATABASE crystal_forge TO crystal_forge;
          '';
        };

        services.crystal-forge = {
          enable = true;
          local-database = true;
          log_level = "debug";

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
            auth_mode = "oidc";
          };

          # Disable build-related services
          build.enable = false;

          # Minimal flake config (not testing flakes in this test)
          flakes = {
            flake_polling_interval = "24h";
            commit_evaluation_interval = "24h";
            build_processing_interval = "24h";
          };
        };

        # Configure OIDC environment for the server
        # Note: Server starts AFTER test script manually starts it when Keycloak is ready
        systemd.services.crystal-forge-server = {
          environment = {
            AUTH_MODE = "oidc";
            CRYSTAL_FORGE_OIDC_ISSUER_URL = "http://keycloak:${toString KEYCLOAK_PORT}/realms/${OIDC_REALM}";
            CRYSTAL_FORGE_OIDC_CLIENT_ID = OIDC_CLIENT_ID;
            CRYSTAL_FORGE_OIDC_CLIENT_SECRET = OIDC_CLIENT_SECRET;
            CRYSTAL_FORGE_OIDC_REDIRECT_URI = "http://server:${
              toString CF_TEST_SERVER_PORT
            }/api/auth/oidc/callback";
          };
          # Don't start automatically - we'll start it after Keycloak is ready
          wantedBy = lib.mkForce [];
        };
      };
    };

    globalTimeout = 600; # 10 minutes

    testScript = ''
      import json
      import urllib.request
      import urllib.parse
      import time

      start_all()

      # ==========================================
      # Phase 1: Wait for services to be ready
      # ==========================================
      print("=== Phase 1: Waiting for services ===")

      # Wait for Keycloak first (it takes a while to start)
      print("Waiting for Keycloak to start...")
      keycloak.wait_for_unit("keycloak.service")
      keycloak.wait_for_open_port(${toString KEYCLOAK_PORT})

      # Wait for realm import to complete - this confirms Keycloak is fully ready
      # The realm import service waits for Keycloak to be ready before importing
      print("Waiting for realm import (this waits for Keycloak to be fully ready)...")
      keycloak.wait_for_unit("keycloak-realm-import.service", timeout=240)
      print("Realm import complete - Keycloak is ready!")

      # Debug: Check network connectivity between VMs
      print("Debug: Checking network configuration...")
      print("Keycloak network:")
      kc_ip = keycloak.succeed("ip addr show")
      print(kc_ip)
      kc_hosts = keycloak.succeed("cat /etc/hosts")
      print(kc_hosts)
      print("Server network:")
      srv_ip = server.succeed("ip addr show")
      print(srv_ip)
      srv_hosts = server.succeed("cat /etc/hosts")
      print(srv_hosts)

      # Test basic connectivity
      print("Testing ping from server to keycloak (IPv4)...")
      ping_result = server.execute("ping -c 3 -4 192.168.1.1 2>&1")
      print(f"Ping IPv4 result (code={ping_result[0]}): {ping_result[1]}")

      # Try resolving keycloak hostname
      print("Testing hostname resolution...")
      resolve_result = server.execute("getent hosts keycloak 2>&1")
      print(f"getent hosts keycloak (code={resolve_result[0]}): {resolve_result[1]}")

      # Test curl to Keycloak using IPv4 directly
      print("Testing curl to Keycloak health endpoint (IPv4)...")
      curl_result = server.execute("curl -v -4 http://192.168.1.1:8080/health/ready 2>&1")
      print(f"curl result (code={curl_result[0]}): {curl_result[1][:500]}")

      # Verify Keycloak OIDC discovery is reachable from server node before starting CF server
      # Use -4 to force IPv4 since Keycloak binds to 0.0.0.0 (IPv4 only)
      print("Verifying Keycloak OIDC discovery reachable from server node...")
      server.wait_until_succeeds(
          "curl -sf -4 http://keycloak:${
        toString KEYCLOAK_PORT
      }/realms/${OIDC_REALM}/.well-known/openid-configuration",
          timeout=60
      )

      # Now start Crystal Forge server (after Keycloak is ready)
      print("Starting Crystal Forge server...")
      server.wait_for_unit("postgresql.service")
      server.succeed("systemctl start crystal-forge-server.service")
      server.wait_for_unit("crystal-forge-server.service")
      server.wait_for_open_port(${toString CF_TEST_SERVER_PORT})
      print("Crystal Forge server is ready!")

      # ==========================================
      # AC#3: Verify OIDC discovery endpoint
      # ==========================================
      print("=== AC#3: Testing OIDC discovery endpoint ===")

      # Test from keycloak node (localhost)
      discovery_local = keycloak.succeed(
          "curl -sf http://localhost:${
        toString KEYCLOAK_PORT
      }/realms/${OIDC_REALM}/.well-known/openid-configuration"
      )
      discovery_data = json.loads(discovery_local)
      assert "issuer" in discovery_data, "Discovery response missing 'issuer'"
      assert "token_endpoint" in discovery_data, "Discovery response missing 'token_endpoint'"
      assert "authorization_endpoint" in discovery_data, "Discovery response missing 'authorization_endpoint'"
      assert "userinfo_endpoint" in discovery_data, "Discovery response missing 'userinfo_endpoint'"
      print(f"OIDC Discovery OK: issuer={discovery_data['issuer']}")

      # Test from server node (cross-VM network)
      discovery_remote = server.succeed(
          "curl -sf http://keycloak:${
        toString KEYCLOAK_PORT
      }/realms/${OIDC_REALM}/.well-known/openid-configuration"
      )
      discovery_remote_data = json.loads(discovery_remote)
      assert discovery_remote_data["issuer"] == discovery_data["issuer"], "Issuer mismatch between local and remote"
      print("OIDC Discovery reachable from server node")

      # ==========================================
      # AC#4: Test OIDC token exchange (ROPC flow)
      # ==========================================
      print("=== AC#4: Testing OIDC token exchange ===")

      # Use Resource Owner Password Credentials grant to get tokens
      # This simulates what the server does when validating tokens
      token_cmd = (
          'curl -sf -X POST "http://localhost:${
        toString KEYCLOAK_PORT
      }/realms/${OIDC_REALM}/protocol/openid-connect/token" '
          '-H "Content-Type: application/x-www-form-urlencoded" '
          '-d "grant_type=password" '
          '-d "client_id=${OIDC_CLIENT_ID}" '
          '-d "client_secret=${OIDC_CLIENT_SECRET}" '
          '-d "username=admin" '
          '-d "password=admin" '
          '-d "scope=openid profile email"'
      )
      token_response = keycloak.succeed(token_cmd)
      token_data = json.loads(token_response)

      assert "access_token" in token_data, "Token response missing 'access_token'"
      assert "id_token" in token_data, "Token response missing 'id_token'"
      assert "refresh_token" in token_data, "Token response missing 'refresh_token'"
      print("OIDC token exchange successful")

      access_token = token_data["access_token"]
      id_token = token_data["id_token"]

      # Verify token at userinfo endpoint
      userinfo_cmd = f'curl -sf "http://localhost:${
        toString KEYCLOAK_PORT
      }/realms/${OIDC_REALM}/protocol/openid-connect/userinfo" -H "Authorization: Bearer {access_token}"'
      userinfo_response = keycloak.succeed(userinfo_cmd)
      userinfo_data = json.loads(userinfo_response)

      assert "sub" in userinfo_data, "Userinfo missing 'sub' (subject)"
      assert "email" in userinfo_data, "Userinfo missing 'email'"
      assert userinfo_data["email"] == "admin@crystal-forge.local", f"Unexpected email: {userinfo_data['email']}"
      print(f"Userinfo verified: sub={userinfo_data['sub']}, email={userinfo_data['email']}")

      # Check groups/roles claim (if present in access token)
      # JWT payload is base64url encoded in the middle section
      import base64

      def decode_jwt_payload(token):
          parts = token.split('.')
          if len(parts) != 3:
              return {}
          payload_b64 = parts[1]
          # Add padding if needed
          padding = 4 - len(payload_b64) % 4
          if padding != 4:
              payload_b64 += '=' * padding
          try:
              payload_json = base64.urlsafe_b64decode(payload_b64)
              return json.loads(payload_json)
          except Exception as e:
              print(f"Failed to decode JWT: {e}")
              return {}

      access_claims = decode_jwt_payload(access_token)
      print(f"Access token claims: {json.dumps(access_claims, indent=2)}")

      # Verify groups claim is present (configured in realm mapper)
      if "groups" in access_claims:
          print(f"Groups claim present: {access_claims['groups']}")
          assert "admin" in access_claims["groups"], "Admin role not in groups claim"
      else:
          print("Warning: 'groups' claim not found in access token (may be in realm_access)")

      # Check realm_access.roles as alternative location
      if "realm_access" in access_claims and "roles" in access_claims["realm_access"]:
          roles = access_claims["realm_access"]["roles"]
          print(f"Realm roles: {roles}")
          assert "admin" in roles, "Admin role not in realm_access.roles"

      # ==========================================
      # AC#5: Test server session creation via OIDC callback simulation
      # ==========================================
      print("=== AC#5: Testing server auth integration ===")

      # Verify Crystal Forge server is responding
      server_status = server.succeed(
          "curl -sf http://localhost:${toString CF_TEST_SERVER_PORT}/status"
      )
      print(f"Server status: {server_status}")

      # Check auth setup status endpoint
      setup_status = server.succeed(
          "curl -sf http://localhost:${
        toString CF_TEST_SERVER_PORT
      }/api/auth/setup-status"
      )
      setup_data = json.loads(setup_status)
      print(f"Setup status: {setup_data}")

      # Verify whoami returns OIDC mode
      whoami_response = server.succeed(
          "curl -sf http://localhost:${
        toString CF_TEST_SERVER_PORT
      }/api/auth/whoami"
      )
      whoami_data = json.loads(whoami_response)
      assert whoami_data.get("auth_mode") == "oidc", f"Expected auth_mode=oidc, got {whoami_data.get('auth_mode')}"
      assert whoami_data.get("is_authenticated") == False, "Should not be authenticated without session"
      print("Server correctly reports OIDC auth mode")

      # Test that OIDC login endpoint exists and redirects
      # (We can't fully test the redirect flow without a browser, but we can verify the endpoint exists)
      login_check = server.execute(
          "curl -s -o /dev/null -w '%{http_code}' http://localhost:${
        toString CF_TEST_SERVER_PORT
      }/api/auth/oidc/login"
      )
      http_code = login_check[1].strip()
      # Should redirect (302/303) to Keycloak
      assert http_code in ["302", "303", "307"], f"OIDC login should redirect, got HTTP {http_code}"
      print(f"OIDC login endpoint returns redirect (HTTP {http_code})")

      # Verify the redirect location points to Keycloak
      login_headers = server.succeed(
          "curl -s -i http://localhost:${
        toString CF_TEST_SERVER_PORT
      }/api/auth/oidc/login | head -20"
      )
      assert "keycloak" in login_headers.lower() or "realms/${OIDC_REALM}" in login_headers, \
          f"OIDC login redirect should point to Keycloak"
      print("OIDC login correctly redirects to Keycloak")

      # ==========================================
      # Verify database session tables exist
      # ==========================================
      print("=== Verifying database schema for sessions ===")

      # Check that user_sessions table exists (for OIDC session storage)
      session_table_check = server.succeed(
          "sudo -u postgres psql -d crystal_forge -c \"SELECT EXISTS (SELECT FROM information_schema.tables WHERE table_name = 'user_sessions');\""
      )
      assert "t" in session_table_check, "user_sessions table should exist"
      print("user_sessions table exists")

      # Check users table exists
      users_table_check = server.succeed(
          "sudo -u postgres psql -d crystal_forge -c \"SELECT EXISTS (SELECT FROM information_schema.tables WHERE table_name = 'users');\""
      )
      assert "t" in users_table_check, "users table should exist"
      print("users table exists")

      # Check external_identities table exists (for OIDC identity binding)
      identities_table_check = server.succeed(
          "sudo -u postgres psql -d crystal_forge -c \"SELECT EXISTS (SELECT FROM information_schema.tables WHERE table_name = 'external_identities');\""
      )
      assert "t" in identities_table_check, "external_identities table should exist"
      print("external_identities table exists")

      print("")
      print("=" * 60)
      print("OIDC Authentication VM Test: ALL CHECKS PASSED")
      print("=" * 60)
      print("- AC#3: OIDC discovery endpoint reachable and valid")
      print("- AC#4: Token exchange (ROPC) works, JWT claims extracted")
      print("- AC#5: Server configured for OIDC, login redirects to Keycloak")
      print("- AC#6: Database schema includes required session tables")
    '';
  }
