{
  pkgs,
  lib,
  ...
}: let
  # Generate a keypair for an agent
  mkKeyPair = name:
    pkgs.runCommand "${name}-keypair" {} ''
      mkdir -p $out
      ${pkgs.crystal-forge.agent.cf-keygen}/bin/cf-keygen -f $out/agent.key
    '';

  # Extract private key
  mkPrivateKey = name: keyPair:
    pkgs.runCommand "${name}-private-key" {} ''
      mkdir -p $out
      cp ${keyPair}/agent.key $out/
    '';

  # Extract public key
  mkPublicKey = name: keyPair:
    lib.strings.removeSuffix "\n" (builtins.readFile "${keyPair}/agent.pub");
in
  # Main function to create an agent with planned actions
  {
    hostname,
    serverHost ? "localhost",
    serverPort ? 8080,
    os ? "25.11",
    kernel ? "6.12.33",
    memoryGb ? 16.0,
    cpuBrand ? "Test CPU",
    cpuCores ? 4,
    primaryIpAddress ? "192.168.1.100",
    primaryMacAddress ? "02:00:00:00:00:01",
    gatewayIp ? "192.168.1.1",
    heartbeatInterval ? 30, # seconds between heartbeats
    actions ? [
      {
        type = "startup";
        derivationPath = "/nix/store/test-system-${hostname}-v1";
      }
      {
        type = "heartbeat";
        delay = 30;
      }
      {
        type = "heartbeat";
        delay = 30;
      }
      {
        type = "config_change";
        derivationPath = "/nix/store/test-system-${hostname}-v2";
        delay = 60;
      }
    ],
  }: let
    keyPair = mkKeyPair hostname;
    privateKey = mkPrivateKey hostname keyPair;
    publicKey = mkPublicKey hostname keyPair;

    # Separate Python script for signing with dependencies
    signScript =
      pkgs.writers.writePython3 "sign-payload" {
        libraries = [pkgs.python3Packages.cryptography];
      } ''
        import base64
        import sys
        from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

        if len(sys.argv) != 2:
            print("Usage: sign-payload <private_key_file>", file=sys.stderr)
            sys.exit(1)

        private_key_file = sys.argv[1]

        with open(private_key_file, 'r') as f:
            private_key_b64 = f.read().strip()

        private_key_bytes = base64.b64decode(private_key_b64)
        private_key = Ed25519PrivateKey.from_private_bytes(private_key_bytes)
        message = sys.stdin.buffer.read()
        signature = private_key.sign(message)
        print(base64.b64encode(signature).decode('ascii'))
      '';

    # Generate the action plan as a bash array - ensure delay is always numeric
    actionPlan =
      lib.concatMapStringsSep "\n" (
        action: let
          actionType = action.type;
          derivPath = action.derivationPath or "/nix/store/test-system-${hostname}-generic";
          delay = toString (action.delay or 0); # Ensure it's a string number
          changeReason =
            if actionType == "startup"
            then "startup"
            else if actionType == "config_change"
            then "config_change"
            else "state_delta";
          endpoint =
            if actionType == "startup" || actionType == "config_change"
            then "state"
            else "heartbeat";
        in "${actionType}|${derivPath}|${changeReason}|${endpoint}|${delay}"
      )
      actions;
  in {
    agent = pkgs.writeShellApplication {
      name = "cf-agent-${hostname}";
      runtimeInputs = with pkgs; [curl coreutils util-linux jq];

      text = ''
        set -euxo pipefail

        echo "Starting agent for ${hostname}..."
        echo "Server: ${serverHost}:${toString serverPort}"
        echo "Planned actions: ${toString (builtins.length actions)}"
        echo ""

        # Generate JSON payload
        generate_payload() {
            local change_reason="$1"
            local derivation_path="$2"
            local timestamp uptime_secs

            timestamp=$(date -u +"%Y-%m-%dT%H:%M:%S.%6NZ")
            uptime_secs=$((RANDOM * 100 + 86400))

            cat << EOF
        {
          "hostname": "${hostname}",
          "change_reason": "$change_reason",
          "timestamp": "$timestamp",
          "derivation_path": "$derivation_path",
          "os": "${os}",
          "kernel": "${kernel}",
          "memory_gb": ${toString memoryGb},
          "uptime_secs": $uptime_secs,
          "cpu_brand": "${cpuBrand}",
          "cpu_cores": ${toString cpuCores},
          "board_serial": "TEST123456789",
          "product_uuid": "$(uuidgen)",
          "rootfs_uuid": "$(uuidgen)",
          "chassis_serial": "CHASSIS123",
          "bios_version": "1.0.0",
          "cpu_microcode": null,
          "network_interfaces": [],
          "primary_mac_address": "${primaryMacAddress}",
          "primary_ip_address": "${primaryIpAddress}",
          "gateway_ip": "${gatewayIp}",
          "selinux_status": null,
          "tpm_present": true,
          "secure_boot_enabled": false,
          "fips_mode": false,
          "agent_version": "0.1.0-test",
          "agent_build_hash": "test-build",
          "nixos_version": "${os}"
        }
        EOF
        }

        # Send message to server
        send_message() {
            local endpoint="$1"
            local payload="$2"
            local action_num="$3"

            echo "[$action_num] Creating ed25519 signature..."

            # The private key is base64-encoded raw ed25519 bytes
            local signature
            if [ -f "${privateKey}/agent.key" ]; then
                echo "[$action_num] Using ed25519 raw key signing..."

                # Use separate Python script for signing
                signature=$(echo -n "$payload" | ${signScript} "${privateKey}/agent.key")
            else
                echo "[$action_num] ⚠️  Private key not found, using mock signature"
                signature=$(echo "test_signature_$(echo -n "$payload" | sha256sum | cut -c1-32)" | base64 -w0)
            fi

            echo "[$action_num] Sending $endpoint message..."

            local response
            response=$(curl -s -w "\n%{http_code}" -X POST \
                -H "Content-Type: application/json" \
                -H "X-Signature: $signature" \
                -H "X-Key-ID: ${hostname}" \
                -d "$payload" \
                "http://${serverHost}:${toString serverPort}/agent/$endpoint")

            local body
            local status
            body=$(echo "$response" | head -n -1)
            status=$(echo "$response" | tail -n 1)

            if [[ "$status" == "200" ]]; then
                echo "[$action_num] ✓ Success"
            else
                echo "[$action_num] ✗ Failed (HTTP $status)"
                echo "[$action_num] Response: $body"
            fi
        }

        # Execute action plan
        action_num=1

        # Read action plan
        while IFS='|' read -r action_type derivation_path change_reason endpoint delay_str; do
            # Safely convert delay to number, default to 0 if invalid
            delay=0
            if [[ "$delay_str" =~ ^[0-9]+$ ]]; then
                delay="$delay_str"
            fi

            if [[ $action_num -gt 1 ]] && [[ $delay -gt 0 ]]; then
                echo "[$action_num] Waiting $delay seconds..."
                sleep "$delay"
            fi

            echo "[$action_num] Executing $action_type action"
            echo "[$action_num] Derivation: $derivation_path"

            payload=$(generate_payload "$change_reason" "$derivation_path")
            send_message "$endpoint" "$payload" "$action_num"

            action_num=$((action_num + 1))
            echo ""
        done <<< "${actionPlan}"

        echo "Agent ${hostname} completed all actions."
      '';
    };

    # Expose the public key for server configuration
    publicKey = publicKey;

    # Private key path (for reference, though it's in the script)
    privateKeyPath = "${privateKey}/agent.key";
  }
