{pkgs}:
pkgs.writeShellApplication {
  name = "cf-agent-test";
  runtimeInputs = with pkgs; [
    curl
    coreutils
    util-linux
    gnugrep
    gnused
  ];

  text = ''
    set -euo pipefail

    usage() {
        echo "Usage: $0 -s SYSTEM_ID -c CONFIG_FILE [heartbeat|state]"
        echo "  -s SYSTEM_ID     System ID from config file"
        echo "  -c CONFIG_FILE   TOML file with system configurations"
        echo "  heartbeat        Send heartbeat (default)"
        echo "  state           Send state change"
        echo ""
        echo "Example: $0 -s test-host-old -c test-systems.toml heartbeat"
        exit 1
    }

    # Parse TOML config for a specific system ID
    parse_system_config() {
        local config_file="$1"
        local system_id="$2"

        # Extract server config
        SERVER_HOST=$(grep -E "^\s*server_host\s*=" "$config_file" | sed 's/.*=\s*"\([^"]*\)".*/\1/')
        SERVER_PORT=$(grep -E "^\s*server_port\s*=" "$config_file" | sed 's/.*=\s*\([0-9]*\).*/\1/')

        # Find the system section for this system ID
        local in_system_section=false
        local current_system_id=""

        while IFS= read -r line; do
            # Check for system section start
            if [[ "$line" =~ ^\[\[systems\]\] ]]; then
                in_system_section=true
                current_system_id=""
                continue
            fi

            # Check for other section start (exit system section)
            if [[ "$line" =~ ^\[.*\] ]] && [[ ! "$line" =~ ^\[\[systems\]\] ]]; then
                in_system_section=false
                continue
            fi

            # If we're in a system section, parse the values
            if [[ "$in_system_section" == true ]]; then
                if [[ "$line" =~ id.*=.*\"(.*)\" ]]; then
                    current_system_id="''${BASH_REMATCH[1]}"
                elif [[ "$current_system_id" == "$system_id" ]]; then
                    # Parse system-specific config
                    if [[ "$line" =~ hostname.*=.*\"(.*)\" ]]; then
                        HOSTNAME="''${BASH_REMATCH[1]}"
                    elif [[ "$line" =~ private_key.*=.*\"(.*)\" ]]; then
                        PRIVATE_KEY="''${BASH_REMATCH[1]}"
                    elif [[ "$line" =~ derivation_path.*=.*\"(.*)\" ]]; then
                        DERIVATION_PATH="''${BASH_REMATCH[1]}"
                    elif [[ "$line" =~ os.*=.*\"(.*)\" ]]; then
                        OS="''${BASH_REMATCH[1]}"
                    elif [[ "$line" =~ kernel.*=.*\"(.*)\" ]]; then
                        KERNEL="''${BASH_REMATCH[1]}"
                    elif [[ "$line" =~ memory_gb.*=.*([0-9.]*) ]]; then
                        MEMORY_GB="''${BASH_REMATCH[1]}"
                    elif [[ "$line" =~ cpu_brand.*=.*\"(.*)\" ]]; then
                        CPU_BRAND="''${BASH_REMATCH[1]}"
                    elif [[ "$line" =~ cpu_cores.*=.*([0-9]*) ]]; then
                        CPU_CORES="''${BASH_REMATCH[1]}"
                    elif [[ "$line" =~ product_uuid.*=.*\"(.*)\" ]]; then
                        PRODUCT_UUID="''${BASH_REMATCH[1]}"
                    elif [[ "$line" =~ primary_ip_address.*=.*\"(.*)\" ]]; then
                        PRIMARY_IP_ADDRESS="''${BASH_REMATCH[1]}"
                    elif [[ "$line" =~ primary_mac_address.*=.*\"(.*)\" ]]; then
                        PRIMARY_MAC_ADDRESS="''${BASH_REMATCH[1]}"
                    elif [[ "$line" =~ gateway_ip.*=.*\"(.*)\" ]]; then
                        GATEWAY_IP="''${BASH_REMATCH[1]}"
                    fi
                fi
            fi
        done < "$config_file"

        # Set defaults if not found
        HOSTNAME="''${HOSTNAME:-unknown-host}"
        DERIVATION_PATH="''${DERIVATION_PATH:-/nix/store/test-system-''${HOSTNAME}}"
        OS="''${OS:-25.11}"
        KERNEL="''${KERNEL:-6.12.33}"
        MEMORY_GB="''${MEMORY_GB:-16.0}"
        CPU_BRAND="''${CPU_BRAND:-Test CPU}"
        CPU_CORES="''${CPU_CORES:-4}"
        PRODUCT_UUID="''${PRODUCT_UUID:-$(uuidgen)}"
        PRIMARY_IP_ADDRESS="''${PRIMARY_IP_ADDRESS:-192.168.1.100}"
        PRIMARY_MAC_ADDRESS="''${PRIMARY_MAC_ADDRESS:-02:00:00:00:00:01}"
        GATEWAY_IP="''${GATEWAY_IP:-192.168.1.1}"
    }

    # Generate JSON payload
    generate_payload() {
        local hostname="$1"
        local change_reason="$2"
        local timestamp=$(date -u +"%Y-%m-%dT%H:%M:%S.%6NZ")
        local uptime_secs=$((RANDOM * 100 + 86400))

        cat << EOF
    {
      "hostname": "$hostname",
      "change_reason": "$change_reason",
      "timestamp": "$timestamp",
      "derivation_path": "$DERIVATION_PATH",
      "os": "$OS",
      "kernel": "$KERNEL",
      "memory_gb": $MEMORY_GB,
      "uptime_secs": $uptime_secs,
      "cpu_brand": "$CPU_BRAND",
      "cpu_cores": $CPU_CORES,
      "board_serial": "TEST123456789",
      "product_uuid": "$PRODUCT_UUID",
      "rootfs_uuid": "$(uuidgen)",
      "chassis_serial": "CHASSIS123",
      "bios_version": "1.0.0",
      "cpu_microcode": null,
      "network_interfaces": [],
      "primary_mac_address": "$PRIMARY_MAC_ADDRESS",
      "primary_ip_address": "$PRIMARY_IP_ADDRESS",
      "gateway_ip": "$GATEWAY_IP",
      "selinux_status": null,
      "tmp_present": true,
      "secure_boot_enabled": false,
      "fips_mode": false,
      "agent_version": "0.1.0-test",
      "agent_build_hash": "test-build",
      "nixos_version": "$OS"
    }
    EOF
    }

    # Sign payload
    sign_payload() {
        local payload="$1"
        local key_b64="$2"

        if [[ -z "$key_b64" ]]; then
            echo "Private key not provided" >&2
            exit 1
        fi

        # For testing, just create a mock signature
        echo "test_signature_$(echo -n "$payload" | sha256sum | cut -c1-32)" | base64 -w0
    }

    # Send to server
    send_message() {
        local endpoint="$1"
        local payload="$2"
        local signature="$3"
        local hostname="$4"

        local url="http://''${SERVER_HOST}:''${SERVER_PORT}/agent/''${endpoint}"

        curl -s -X POST \
            -H "Content-Type: application/json" \
            -H "X-Signature: $signature" \
            -H "X-Key-ID: $hostname" \
            -d "$payload" \
            "$url"
    }

    # Main
    main() {
        local system_id=""
        local config_file=""
        local action="heartbeat"

        while [[ $# -gt 0 ]]; do
            case $1 in
                -s) system_id="$2"; shift 2 ;;
                -c) config_file="$2"; shift 2 ;;
                heartbeat|state) action="$1"; shift ;;
                *) usage ;;
            esac
        done

        if [[ -z "$system_id" || -z "$config_file" ]]; then
            usage
        fi

        if [[ ! -f "$config_file" ]]; then
            echo "Config file not found: $config_file" >&2
            exit 1
        fi

        parse_system_config "$config_file" "$system_id"

        local change_reason="heartbeat"
        local endpoint="heartbeat"

        if [[ "$action" == "state" ]]; then
            change_reason="config_change"
            endpoint="state"
        fi

        local payload=$(generate_payload "$HOSTNAME" "$change_reason")
        local signature=$(sign_payload "$payload" "$PRIVATE_KEY")

        send_message "$endpoint" "$payload" "$signature" "$HOSTNAME"
    }

    main "$@"
  '';
}
