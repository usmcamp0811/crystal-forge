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
    finalPrivateKeyString =
      if privateKeyString != null
      then privateKeyString
      else builtins.readFile "${autoPrivateKey}/agent.key";

    publicKey =
      if publicKeyString != null
      then publicKeyString
      else autoPublicKey;

    # Generate execution script using test-agent
    executeTimeline =
      lib.concatMapStringsSep "\n" (action: let
        actionType = action.type;
        derivPath = action.derivationPath or "/nix/store/test-system-${hostname}-generic";
        delay = action.delay or 0;
        daysBack = action.daysBack or 0;
        hoursBack = action.hoursBack or 0;

        changeReason =
          if actionType == "startup"
          then "startup"
          else if actionType == "config_change"
          then "config_change"
          else if actionType == "heartbeat"
          then "heartbeat"
          else "state_delta";

        # Calculate historical timestamp
        timestampCalc =
          if daysBack != 0 || hoursBack != 0
          then ''$(date -u -d "@$(($(date +%s) - ${toString daysBack} * 86400 - ${toString hoursBack} * 3600))" '+%Y-%m-%dT%H:%M:%S.000000Z')''
          else ''''; # Empty means current time
      in ''
        # Execute ${actionType} action
        echo "Executing ${actionType} for ${hostname}"
        ${
          if delay > 0
          then "sleep ${toString delay}"
          else ""
        }

        ${pkgs.crystal-forge.default}/bin/test-agent \
          --hostname "${hostname}" \
          --change-reason "${changeReason}" \
          --derivation "${derivPath}" \
          ${
          if timestampCalc != ""
          then ''--timestamp "${timestampCalc}"''
          else ""
        } \
          --server-host "${serverHost}" \
          --server-port "${toString serverPort}" \
          --private-key "${finalPrivateKeyString}" \
          ${
          if os != "25.11"
          then ''--os "${os}"''
          else ""
        } \
          ${
          if kernel != "6.12.33"
          then ''--kernel "${kernel}"''
          else ""
        } \
          ${
          if memoryGb != 16.0
          then ''--memory-gb "${toString memoryGb}"''
          else ""
        } \
          ${
          if cpuBrand != "Test CPU"
          then ''--cpu-brand "${cpuBrand}"''
          else ""
        } \
          ${
          if cpuCores != 4
          then ''--cpu-cores "${toString cpuCores}"''
          else ""
        }
        echo ""
      '')
      actions;
  in {
    agent = pkgs.writeShellApplication {
      name = "cf-agent-${hostname}";
      runtimeInputs = with pkgs; [coreutils util-linux];

      text = ''
        set -euo pipefail

        echo "Starting test agent for ${hostname}..."
        echo "Server: ${serverHost}:${toString serverPort}"
        echo "Planned actions: ${toString (builtins.length actions)}"
        echo ""

        ${executeTimeline}

        echo "Agent ${hostname} completed all actions."
      '';
    };

    # Expose everything needed for orchestrator
    inherit actions hostname;
    privateKeyString = finalPrivateKeyString;
    inherit serverHost serverPort;
    publicKey = publicKey;
    privateKeyPath =
      if privateKeyString != null
      then null # No physical path for string keys
      else "${autoPrivateKey}/agent.key";
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
    agents,
    timeScale ? 0.01,
    sqlJobsPackage ? pkgs.crystal-forge.run-postgres-jobs,
    simulationDays ? 7, # How many days to simulate
  }: let
    # Time constants based on timeScale parameter
    minute = 60.0 * timeScale;
    hour = 60.0 * minute;
    day = 24.0 * hour;

    # Extract and merge all actions into timeline with agent info
    allAgentActions = lib.flatten (map (
        agent:
          map (action:
            action
            // {
              hostname = agent.hostname;
              privateKey = agent.privateKeyString;
              serverHost = agent.serverHost or "localhost";
              serverPort = agent.serverPort or 3445;
            })
          agent.actions
      )
      agents);

    # Add SQL job events at midnight boundaries (one for each day)
    midnightJobs = map (day: {
      type = "sql_job";
      daysBack = simulationDays - day;
      hoursBack = 0;
      dayNum = day;
    }) (range 0 (simulationDays - 1));

    # Sort all events chronologically (oldest first - largest daysBack first)
    allEvents = lib.sort (
      a: b: let
        aTime = (a.daysBack or 0) * 24 + (a.hoursBack or 0);
        bTime = (b.daysBack or 0) * 24 + (b.hoursBack or 0);
      in
        aTime > bTime
    ) (allAgentActions ++ midnightJobs);

    # Generate execution script for timeline
    executeTimeline =
      lib.concatMapStringsSep "\n" (
        event:
          if event.type == "sql_job"
          then ''
            echo "🌙 [Day ${toString event.dayNum}] Running midnight SQL jobs..."
            echo "  Simulated date: $(date -u -d "@$((sim_start_time - ${toString event.daysBack} * 86400))" '+%Y-%m-%d %H:%M:%S UTC')"
            ${sqlJobsPackage}/bin/run-postgres-jobs
            echo ""
          ''
          else let
            # Calculate total seconds back from now (convert fractional hours to integer seconds)
            daysBackSeconds = (event.daysBack or 0) * 86400;
            hoursBackSeconds = builtins.floor ((event.hoursBack or 0) * 3600);
            totalSecondsBack = daysBackSeconds + hoursBackSeconds;

            # Calculate timestamp for this event using integer seconds
            timestampCalculation = ''$(date -u -d "@$((sim_start_time - ${toString totalSecondsBack}))" '+%Y-%m-%dT%H:%M:%S.000000Z')'';

            # Determine change reason and endpoint
            changeReason =
              if event.type == "startup"
              then "startup"
              else if event.type == "config_change"
              then "config_change"
              else if event.type == "heartbeat"
              then "heartbeat"
              else "state_delta";
          in ''
            # Execute ${event.type} for ${event.hostname} (${toString (event.daysBack or 0)}d ${toString (event.hoursBack or 0)}h back)
            echo "Executing ${event.type} for ${event.hostname} at ${timestampCalculation}"
            ${pkgs.crystal-forge.default}/bin/test-agent \
              --hostname "${event.hostname}" \
              --change-reason "${changeReason}" \
              --derivation "${event.derivationPath or ""}" \
              --timestamp "${timestampCalculation}" \
              --server-host "${event.serverHost}" \
              --server-port "${toString event.serverPort}" \
              --private-key "${event.privateKey}"

            # Respect timing delays (scaled)
            ${
              if (event.delay or 0) > 0
              then "sleep ${toString event.delay}"
              else ""
            }
          ''
      )
      allEvents;
  in
    pkgs.writeShellApplication {
      name = "weekly-orchestrator";
      runtimeInputs = with pkgs; [coreutils util-linux];
      text = ''
        set -euo pipefail

        echo "🚀 Starting Timeline Orchestrator..."
        echo "Time scale: ${toString timeScale} (${toString (1.0 / timeScale)}x faster than real-time)"
        echo "Simulation days: ${toString simulationDays}"
        echo "Total events: ${toString (length allEvents)}"
        echo ""

        # Record simulation start time (this becomes "now" in our timeline)
        sim_start_time=$(date +%s)
        sim_start_date=$(date -u -d "@$sim_start_time" '+%Y-%m-%d %H:%M:%S UTC')

        echo "Simulation timeline:"
        echo "  Start: $(date -u -d "@$((sim_start_time - ${toString simulationDays} * 86400))" '+%Y-%m-%d %H:%M:%S UTC') (${toString simulationDays} days ago)"
        echo "  End:   $sim_start_date (now)"
        echo ""

        # Execute all events in chronological order
        ${executeTimeline}

        echo "✅ Timeline simulation complete!"
        echo "Simulated ${toString simulationDays} days of monitoring data ending at $sim_start_date"
      '';
    };
}
