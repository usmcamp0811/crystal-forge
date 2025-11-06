{lib, ...}:
with lib; rec {
  # Generate a keypair for an agent
  mkKeyPair = {
    pkgs,
    name,
  }:
    pkgs.runCommand "${name}-keypair" {} ''
      mkdir -p $out
      ${pkgs.crystal-forge.defautl.cf-keygen}/bin/cf-keygen -f $out/agent.key
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

      # Calculate how far back this day is from "now" in total seconds
      daysBackFromNow = 6 - dayNum; # Day 0 = 6 days ago, Day 6 = 0 days ago (today)
      dayStartSecondsBack = daysBackFromNow * 86400; # Start of this day in seconds back

      # Regular heartbeats throughout the day - use current derivation
      heartbeats = map (i: let
        # Calculate seconds within the day for this heartbeat (spread evenly)
        secondsIntoDay = i * (86400 / dailyHeartbeats); # 86400 seconds per day
        totalSecondsBack = dayStartSecondsBack + builtins.floor secondsIntoDay;
      in {
        type = "heartbeat";
        derivationPath = currentDerivation;
        delay =
          if dayNum == 0 && i == 0
          then 0
          else heartbeatInterval;
        secondsBack = totalSecondsBack;
        # Keep legacy fields for compatibility but they won't be the primary calculation
        daysBack = daysBackFromNow;
        hoursBack = secondsIntoDay / 3600.0;
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

          updateSecondsBack = dayStartSecondsBack + (2 * 3600); # 2 hours into the day
        in
          optional updateDays {
            type = "config_change";
            derivationPath = elemAt updateDerivations updateIndex;
            delay =
              if endTimeNow
              then heartbeatInterval
              else (2 * hour);
            secondsBack = updateSecondsBack;
            daysBack = daysBackFromNow;
            hoursBack = 2; # Updates happen 2 hours into the day
          }
        else [];

      # Deterministic emergency restart on day 4
      emergencySecondsBack = dayStartSecondsBack + (8 * 3600); # 8 hours into day 4
      emergencies = optional (dayNum == 4 && emergencyRestarts > 0) {
        type = "startup";
        derivationPath = startDerivation;
        delay =
          if endTimeNow
          then heartbeatInterval
          else (8 * hour);
        secondsBack = emergencySecondsBack;
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
        secondsBack = 7 * 86400; # 7 days ago in seconds
        daysBack = 7; # Initial startup was 7 days ago
        hoursBack = 0;
      }
    ]
    ++ allDayActions;

  # Daily orchestrator - runs a single 24-hour period starting and ending at midnight
  mkDailyOrchestrator = {
    pkgs,
    agents,
    daysBack ? 1, # How many days ago to simulate (1 = yesterday)
    timeScale ? 0.01,
    sqlJobsPackage ? pkgs.crystal-forge.run-postgres-jobs,
    dailyHeartbeats ? 96, # Every 15 minutes = 96 per day
    agentConfigChanges ? {}, # Attrset of hostname -> [{derivationPath, hour}] for config changes
    agentRestarts ? {}, # Attrset of hostname -> [{derivationPath, hour}] for restarts
  }: let
    # Helper function for absolute value
    abs = x:
      if x < 0
      then -x
      else x;

    # Time constants based on timeScale parameter
    minute = 60.0 * timeScale;
    hour = 60.0 * minute;

    # Calculate the start of the target day (midnight) in seconds back from now
    dayStartSecondsBack = daysBack * 86400;
    dayEndSecondsBack = (daysBack - 1) * 86400;

    # Generate actions for each agent for this specific day
    dailyAgentActions = lib.flatten (map (agent: let
      # Get the current system derivation for this agent (use first available or fallback)
      currentDerivation =
        if agent ? currentDerivation
        then agent.currentDerivation
        else if agent ? startDerivation
        then agent.startDerivation
        else "/nix/store/default-system-${agent.hostname}";

      # Get config changes and restarts for this specific agent
      agentConfigChangesList = agentConfigChanges.${agent.hostname} or [];
      agentRestartsList = agentRestarts.${agent.hostname} or [];

      # Generate heartbeats throughout the day
      heartbeatActions = map (i: let
        # Calculate seconds within the day for this heartbeat (spread evenly)
        secondsIntoDay = i * (86400 / dailyHeartbeats); # 86400 seconds per day
        totalSecondsBack = dayStartSecondsBack - builtins.floor secondsIntoDay;
      in {
        type = "heartbeat";
        derivationPath = currentDerivation;
        secondsBack = totalSecondsBack;
        hostname = agent.hostname;
        privateKey = agent.privateKeyString;
        serverHost = agent.serverHost or "localhost";
        serverPort = agent.serverPort or 3445;
        delay =
          if i == 0
          then 0
          else (15 * minute); # 15 minute intervals
      }) (range 0 (dailyHeartbeats - 1));

      # Generate config change actions for this agent
      configChangeActions =
        map (change: {
          type = "config_change";
          derivationPath = change.derivationPath;
          secondsBack = dayStartSecondsBack - (change.hour * 3600);
          hostname = agent.hostname;
          privateKey = agent.privateKeyString;
          serverHost = agent.serverHost or "localhost";
          serverPort = agent.serverPort or 3445;
          delay = 30; # Brief delay between config changes
        })
        agentConfigChangesList;

      # Generate restart actions for this agent
      restartActions =
        map (restart: {
          type = "startup";
          derivationPath = restart.derivationPath;
          secondsBack = dayStartSecondsBack - (restart.hour * 3600);
          hostname = agent.hostname;
          privateKey = agent.privateKeyString;
          serverHost = agent.serverHost or "localhost";
          serverPort = agent.serverPort or 3445;
          delay = 60; # Longer delay for restarts
        })
        agentRestartsList;
    in
      heartbeatActions ++ configChangeActions ++ restartActions)
    agents);

    # Add SQL job at the end of the day (just before next midnight)
    sqlJobEvent = {
      type = "sql_job";
      secondsBack = dayEndSecondsBack + 1; # 1 second before the next day starts
      daysBack = daysBack - 1;
      hoursBack = 23.999; # Just before midnight
    };

    # Sort all events chronologically (oldest first - largest seconds back first)
    allEvents = lib.sort (
      a: b: let
        aSecondsBack =
          if a ? secondsBack
          then a.secondsBack
          else (a.daysBack or 0) * 86400 + builtins.floor ((a.hoursBack or 0) * 3600);
        bSecondsBack =
          if b ? secondsBack
          then b.secondsBack
          else (b.daysBack or 0) * 86400 + builtins.floor ((b.hoursBack or 0) * 3600);
      in
        aSecondsBack > bSecondsBack
    ) (dailyAgentActions ++ [sqlJobEvent]);

    # Generate execution script for the day
    executeTimeline =
      lib.concatMapStringsSep "\n" (
        event:
          if event.type == "sql_job"
          then ''
            echo "🌙 End of day - Running SQL jobs..."
            echo "  Target date: $(date -u -d "@$((sim_start_time - ${toString event.secondsBack}))" '+%Y-%m-%d %H:%M:%S UTC')"
            ${sqlJobsPackage}/bin/run-postgres-jobs
            echo ""
          ''
          else let
            # Calculate timestamp for this event
            timestampCalculation = ''$(date -u -d "@$((sim_start_time - ${toString event.secondsBack}))" '+%Y-%m-%dT%H:%M:%S.000000Z')'';

            # Debug info
            debugInfo = "${toString event.secondsBack}s back";

            # Determine change reason
            changeReason =
              if event.type == "startup"
              then "startup"
              else if event.type == "config_change"
              then "config_change"
              else if event.type == "heartbeat"
              then "heartbeat"
              else "state_delta";
          in ''
            # Execute ${event.type} for ${event.hostname} (${debugInfo})
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

    # Calculate the target date for display
    targetDateCalc = ''$(date -u -d "@$((sim_start_time - ${toString dayStartSecondsBack}))" '+%Y-%m-%d')'';
  in
    pkgs.writeShellApplication {
      name = "daily-orchestrator-${toString daysBack}";
      runtimeInputs = with pkgs; [coreutils util-linux];
      text = ''
        set -euo pipefail

        echo "🌅 Starting Daily Orchestrator..."
        echo "Time scale: ${toString timeScale} (${toString (1.0 / timeScale)}x faster than real-time)"
        echo "Days back: ${toString daysBack}"
        echo "Daily heartbeats: ${toString dailyHeartbeats}"
        echo "Agent config changes: ${toString (lib.attrNames agentConfigChanges)}"
        echo "Agent restarts: ${toString (lib.attrNames agentRestarts)}"
        echo "Total events: ${toString (length allEvents)}"
        echo ""

        # Record simulation start time (this becomes "now" in our timeline)
        sim_start_time=$(date +%s)
        target_date=${targetDateCalc}

        echo "Simulating day: $target_date"
        echo "  Period: $(date -u -d "@$((sim_start_time - ${toString dayStartSecondsBack}))" '+%Y-%m-%d 00:00:00 UTC') to $(date -u -d "@$((sim_start_time - ${toString dayEndSecondsBack}))" '+%Y-%m-%d 00:00:00 UTC')"
        echo ""

        # Execute all events in chronological order
        ${executeTimeline}

        echo "✅ Daily simulation complete for $target_date!"
        echo "Simulated 24-hour period with SQL jobs at end of day"
      '';
    };

  mkWeeklyOrchestrator = {
    pkgs,
    agents,
    timeScale ? 0.01,
    sqlJobsPackage ? pkgs.crystal-forge.run-postgres-jobs,
    simulationDays ? 7, # How many days to simulate
    dailyHeartbeats ? 96, # Every 15 minutes = 96 per day
    agentConfigChanges ? {}, # Attrset of hostname -> [{derivationPath, hour}] for config changes per day
    agentRestarts ? {}, # Attrset of hostname -> [{derivationPath, hour}] for restarts per day
  }: let
    # Generate daily orchestrators for each day in the simulation period
    dailyOrchestrators = map (
      dayOffset:
        mkDailyOrchestrator {
          inherit pkgs agents timeScale sqlJobsPackage dailyHeartbeats agentConfigChanges agentRestarts;
          daysBack = simulationDays - dayOffset; # Start from furthest back day
        }
    ) (range 0 (simulationDays - 1));

    # Create execution script that runs each daily orchestrator in sequence
    executeWeeklyTimeline = lib.concatMapStringsSep "\n" (dayOrchestratorIndex: let
      dayOffset = dayOrchestratorIndex;
      daysBack = simulationDays - dayOffset;
      orchestrator = elemAt dailyOrchestrators dayOrchestratorIndex;
    in ''
      echo "📅 Day ${toString (dayOffset + 1)} of ${toString simulationDays} (${toString daysBack} days ago)"
      echo "Running daily orchestrator..."
      ${orchestrator}/bin/daily-orchestrator-${toString daysBack}
      echo ""
    '') (range 0 (simulationDays - 1));
  in
    pkgs.writeShellApplication {
      name = "weekly-orchestrator";
      runtimeInputs = with pkgs; [coreutils util-linux];
      text = ''
        set -euo pipefail

        echo "🚀 Starting Weekly Timeline Orchestrator..."
        echo "Time scale: ${toString timeScale} (${toString (1.0 / timeScale)}x faster than real-time)"
        echo "Simulation days: ${toString simulationDays}"
        echo "Daily heartbeats: ${toString dailyHeartbeats}"
        echo "Agents with config changes: ${toString (lib.attrNames agentConfigChanges)}"
        echo "Agents with restarts: ${toString (lib.attrNames agentRestarts)}"
        echo "Running ${toString simulationDays} consecutive daily simulations..."
        echo ""

        # Record overall simulation start time
        overall_start_time=$(date +%s)
        overall_start_date=$(date -u -d "@$overall_start_time" '+%Y-%m-%d %H:%M:%S UTC')

        echo "Weekly simulation timeline:"
        echo "  Start: $(date -u -d "@$((overall_start_time - ${toString simulationDays} * 86400))" '+%Y-%m-%d %H:%M:%S UTC') (${toString simulationDays} days ago)"
        echo "  End:   $overall_start_date (now)"
        echo ""

        # Execute daily orchestrators in chronological order (oldest to newest)
        ${executeWeeklyTimeline}

        echo "✅ Weekly timeline simulation complete!"
        echo "Simulated ${toString simulationDays} consecutive days ending at $overall_start_date"
        echo "Each day included complete agent actions plus end-of-day SQL jobs"
      '';
    };
}
