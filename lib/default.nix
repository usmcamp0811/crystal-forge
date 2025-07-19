{lib, ...}:
with lib; rec {
  # Generate a keypair for an agent
  mkKeyPair = {
    pkgs,
    name,
  }:
    pkgs.runCommand "${name}-keypair" {} ''
      mkdir -p $out
      ${pkgs.crystal-forge.agent.cf-keygen}/bin/cf-keygen -f $out/agent.key
    '';

  # Extract private key
  mkPrivateKey = {
    pkgs,
    name,
    keyPair,
  }:
    pkgs.runCommand "${name}-private-key" {} ''
      mkdir -p $out
      cp ${keyPair}/agent.key $out/
    '';

  # Extract public key
  mkPublicKey = {
    pkgs,
    name,
    keyPair,
  }:
    lib.strings.removeSuffix "\n" (builtins.readFile "${keyPair}/agent.pub");

  mkAgent = {
    pkgs,
    hostname,
    serverHost ? "localhost",
    serverPort ? 3445,
    os ? "25.11",
    kernel ? "6.12.33",
    memoryGb ? 16.0,
    cpuBrand ? "Test CPU",
    cpuCores ? 4,
    primaryIpAddress ? "192.168.1.100",
    primaryMacAddress ? "02:00:00:00:00:01",
    gatewayIp ? "192.168.1.1",
    heartbeatInterval ? 30, # seconds between heartbeats
    # Optional key overrides - if null, auto-generate
    privateKeyString ? null, # Private key content as string (base64 encoded)
    publicKeyString ? null, # Public key as string
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
    # Generate keys if not provided
    autoKeyPair = mkKeyPair {
      inherit pkgs;
      name = hostname;
    };
    autoPrivateKey = mkPrivateKey {
      inherit pkgs;
      name = hostname;
      keyPair = autoKeyPair;
    };
    autoPublicKey = mkPublicKey {
      inherit pkgs;
      name = hostname;
      keyPair = autoKeyPair;
    };

    # Use provided keys or fall back to auto-generated ones
    privateKey =
      if privateKeyString != null
      then
        pkgs.runCommand "${hostname}-private-key-wrapper" {} ''
          mkdir -p $out
          echo -n "${privateKeyString}" > $out/agent.key
        ''
      else autoPrivateKey;

    publicKey =
      if publicKeyString != null
      then publicKeyString
      else autoPublicKey;

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
          # Calculate historical timing
          daysBack = toString (action.daysBack or 0);
          hoursBack = toString (action.hoursBack or 0);
          changeReason =
            if actionType == "startup"
            then "startup"
            else if actionType == "config_change"
            then "config_change"
            else if actionType == "heartbeat"
            then "heartbeat"
            else "state_delta";
          endpoint =
            if actionType == "startup" || actionType == "config_change"
            then "state"
            else "heartbeat";
        in "${actionType}|${derivPath}|${changeReason}|${endpoint}|${delay}|${daysBack}|${hoursBack}"
      )
      actions;
  in {
    agent = pkgs.writeShellApplication {
      name = "cf-agent-${hostname}";
      runtimeInputs = with pkgs; [curl coreutils util-linux jq];

      text = ''
        set -euo pipefail

        echo "Starting agent for ${hostname}..."
        echo "Server: ${serverHost}:${toString serverPort}"
        echo "Planned actions: ${toString (builtins.length actions)}"
        echo ""

        # Generate JSON payload with historical timestamp
        generate_payload() {
            local change_reason="$1"
            local derivation_path="$2"
            local days_back="$3"
            local hours_back="$4"
            local timestamp uptime_secs

            # Calculate historical timestamp
            if [[ "$days_back" != "0" || "$hours_back" != "0" ]]; then
                local total_seconds_back=$((days_back * 86400 + hours_back * 3600))
                timestamp=$(date -u -d "@$(($(date +%s) - total_seconds_back))" +"%Y-%m-%dT%H:%M:%S.%6NZ")
            else
                timestamp=$(date -u +"%Y-%m-%dT%H:%M:%S.%6NZ")
            fi

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
          "product_uuid": "test-uuid-${hostname}",
          "rootfs_uuid": "test-rootfs-${hostname}",
          "chassis_serial": "CHASSIS123",
          "bios_version": "1.0.0",
          "cpu_microcode": null,
          "network_interfaces": [],
          "primary_mac_address": "${primaryMacAddress}",
          "primary_ip_address": "${primaryIpAddress}",
          "gateway_ip": "${gatewayIp}",
          "selinux_status": null,
          "tmp_present": true,
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
        while IFS='|' read -r action_type derivation_path change_reason endpoint delay_str days_back_str hours_back_str; do
            # Safely convert delay to number, default to 0 if invalid
            delay=0
            if [[ "$delay_str" =~ ^[0-9]+$ ]]; then
                delay="$delay_str"
            fi

            # Extract timing info for historical timestamps
            days_back=0
            hours_back=0
            if [[ "$days_back_str" =~ ^[0-9]+$ ]]; then
                days_back="$days_back_str"
            fi
            if [[ "$hours_back_str" =~ ^[0-9]+$ ]]; then
                hours_back="$hours_back_str"
            fi

            if [[ $action_num -gt 1 ]] && [[ $delay -gt 0 ]]; then
                echo "[$action_num] Waiting $delay seconds..."
                sleep "$delay"
            fi

            payload=$(generate_payload "$change_reason" "$derivation_path" "$days_back" "$hours_back")

            # Extract timestamp from payload for debugging
            timestamp_debug=$(echo "$payload" | grep '"timestamp"' | sed 's/.*"timestamp": *"\([^"]*\)".*/\1/')

            echo "[$action_num] DEBUG: action_type=$action_type, change_reason=$change_reason, endpoint=$endpoint, days_back=$days_back, hours_back=$hours_back, timestamp=$timestamp_debug" >&2
            echo "[$action_num] Executing $action_type action"
            echo "[$action_num] Derivation: $derivation_path"

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
  };

  # Helper function to generate realistic heartbeat intervals
  mkHeartbeats = count: interval:
    map (i: {
      type = "heartbeat";
      delay = interval;
    }) (range 1 count);

  # Helper function to create a realistic week-long monitoring scenario
  mkWeeklyActions = {
    startDerivation,
    updateDerivations ? [],
    dailyHeartbeats ? 96, # Every 15 minutes = 96 per day
    weeklyUpdates ? 2,
    emergencyRestarts ? 1,
    timeScale ? 1.0, # Real seconds per simulated minute (1.0 = real-time, 0.1 = 10x faster, 0.001 = 1000x faster)
    endTimeNow ? true, # If true, compress all delays to finish quickly
  }: let
    # Time constants based on timeScale parameter
    minute = 60.0 * timeScale;
    hour = 60 * minute;
    day = 24 * hour;

    # Heartbeat interval always respects timeScale (15 simulated minutes)
    heartbeatInterval = 15 * minute;

    # Track the current system state for each day
    getCurrentDerivation = dayNum:
      if dayNum == 0
      then startDerivation
      else if dayNum > 0 && (length updateDerivations > 0)
      then let
        updateIndex = mod (dayNum - 1) (length updateDerivations);
      in
        elemAt updateDerivations updateIndex
      else startDerivation;

    # Generate actions for each day
    generateDayActions = dayNum: let
      currentDerivation = getCurrentDerivation dayNum;

      # Calculate how far back this day is from "now"
      daysBackFromNow = 6 - dayNum; # Day 0 = 6 days ago, Day 6 = 0 days ago (today)

      # Regular heartbeats throughout the day - use current derivation
      heartbeats = map (i: {
        type = "heartbeat";
        derivationPath = currentDerivation;
        delay =
          if dayNum == 0 && i == 0
          then 0
          else heartbeatInterval;
        daysBack = daysBackFromNow;
        hoursBack = i * (24 / dailyHeartbeats); # Spread throughout the day
      }) (range 0 (dailyHeartbeats - 1));

      # Deterministic system updates - spread evenly through the week
      updates =
        if (length updateDerivations > 0) && dayNum > 0 && weeklyUpdates > 0
        then let
          # Calculate which update to use based on day
          updateIndex = mod (dayNum - 1) (length updateDerivations);
          # Determine if this day should have an update based on weeklyUpdates
          # Spread updates evenly across the 7-day week
          updateDays =
            if weeklyUpdates >= 7
            then
              # If weeklyUpdates >= 7, update every day
              true
            else
              # Otherwise, spread updates evenly: days 1, 3, 5 for weeklyUpdates=3, etc.
              let
                dayInterval = 6 / weeklyUpdates; # 6 days to spread across (excluding day 0)
              in
                mod dayNum (builtins.ceil dayInterval) == 1;
        in
          optional updateDays {
            type = "config_change";
            derivationPath = elemAt updateDerivations updateIndex;
            delay =
              if endTimeNow
              then heartbeatInterval
              else (2 * hour);
            daysBack = daysBackFromNow;
            hoursBack = 2; # Updates happen 2 hours into the day
          }
        else [];

      # Deterministic emergency restart on day 4
      emergencies = optional (dayNum == 4 && emergencyRestarts > 0) {
        type = "startup";
        derivationPath = startDerivation;
        delay =
          if endTimeNow
          then heartbeatInterval
          else (8 * hour);
        daysBack = daysBackFromNow;
        hoursBack = 8; # Emergency at 8 hours into the day
      };
    in
      heartbeats ++ updates ++ emergencies;

    # Generate 7 days worth of actions
    allDayActions = concatMap generateDayActions (range 0 6);
  in
    [
      {
        type = "startup";
        derivationPath = startDerivation;
        daysBack = 7; # Initial startup was 7 days ago
        hoursBack = 0;
      }
    ]
    ++ allDayActions;

  mkWeeklyOrchestrator = {
    pkgs,
    agents, # List of agent definitions
    timeScale ? 0.01,
    sqlJobsPackage ? pkgs.crystal-forge.run-postgres-jobs,
  }: let
    # Time constants based on timeScale parameter
    minute = 60.0 * timeScale;
    hour = 60.0 * minute;
    day = 24.0 * hour;
    # Round midnightInterval down to avoid bash float errors
    midnightInterval = builtins.floor day;
    agentStartScript =
      lib.concatMapStringsSep "\n" (agent: ''
        echo "Starting agent: ${agent.agent.name}"
        ${agent.agent}/bin/${agent.agent.name} &
        agent_pids+=($!)
      '')
      agents;
  in
    pkgs.writeShellApplication {
      name = "weekly-orchestrator";
      runtimeInputs = with pkgs; [coreutils util-linux];
      text = ''
        set -euo pipefail
        echo "🚀 Starting Weekly Orchestrator..."
        echo "Time scale: ${toString timeScale}"
        echo "Agents: ${toString (length agents)}"
        echo "Midnight interval: ${toString midnightInterval} seconds"
        echo ""
        agent_pids=()
        ${agentStartScript}
        echo "All agents started. PIDs: ''${agent_pids[*]}"
        echo ""
        run_sql_jobs() {
          local day_num="$1"
          echo "🌙 [Day $day_num] Running midnight SQL jobs..."
          if command -v ${sqlJobsPackage}/bin/run-postgres-jobs >/dev/null 2>&1; then
            ${sqlJobsPackage}/bin/run-postgres-jobs
            echo "✅ [Day $day_num] SQL jobs completed"
          else
            echo "⚠️  [Day $day_num] SQL jobs package not found, skipping"
          fi
          echo ""
        }
        day_counter=0
        next_midnight=$(date +%s)
        next_midnight=$((next_midnight + ${toString midnightInterval}))
        echo "⏰ Next midnight job scheduled for: $(date -d @$next_midnight)"
        echo ""
        while true; do
          current_time=$(date +%s)
          if [[ $current_time -ge $next_midnight ]]; then
            day_counter=$((day_counter + 1))
            run_sql_jobs "$day_counter"
            next_midnight=$((next_midnight + ${toString midnightInterval}))
            echo "⏰ Next midnight job scheduled for: $(date -d @$next_midnight)"
            echo ""
          fi
          active_agents=0
          for pid in "''${agent_pids[@]}"; do
            if kill -0 "$pid" 2>/dev/null; then
              active_agents=$((active_agents + 1))
            fi
          done
          if [[ $active_agents -eq 0 ]]; then
            echo "🏁 All agents completed"
            break
          fi
          sleep 1
        done
        echo "🎯 Running final SQL jobs..."
        run_sql_jobs "final"
        echo "✅ Weekly simulation complete!"
      '';
    };
}
