---
type: Operator Guide
title: "Step 6: Deploy Agent"
description: "Walks through deploying the Crystal Forge agent on a managed NixOS system: module configuration, key placement, verification commands, expected behavior after connection, and the administrator acknowledgement that completes the step."
tags:
  - crystal-forge
  - onboarding
  - agent
  - nixos-module
  - heartbeat
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T21:00:00-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/onboarding-guide.md at commit 3b23d36f"
    title: "Crystal Forge Onboarding Guide"
---

# Step 6: Deploy Agent

The **Crystal Forge Agent** runs on each managed NixOS system as `root`. It reports system state, receives deployment targets, and activates them. Without the agent running, Crystal Forge cannot monitor or deploy to the system.

## Before you start

You need a registered system and the private key that matches the public key you saved during Step 5. To start tracking the system, you must:

1. **Enable the Crystal Forge agent module** in the system's NixOS configuration.
2. **Apply the configuration** on the target system (`nixos-rebuild switch`).
3. **Verify the agent service is running** (`systemctl status crystal-forge-agent`).

## Enabling the Agent in NixOS Config

On the **target system** (the NixOS host you want to manage), add this to its configuration:

```nix
{
  services.crystal-forge.client = {
    enable = true;
    
    # Crystal Forge server connection
    server_host = "crystal-forge.example.com";
    server_port = 3000;
    
    # Private key (the one you generated during system registration).
    # Use a string path to a file outside the Nix store.
    private_key = "/var/lib/crystal-forge/host.key";
  };
}
```

**Save the private key** to the target system. The agent service runs as `root`, so `root` owns the key file. The `crystal-forge` user does not exist on a host that runs only the agent.

```bash
# On the target system (for example, web-server-1)
sudo install -d -m 0700 -o root -g root /var/lib/crystal-forge
# The shell below runs as root with umask 077, so the file is never group- or world-readable.
sudo sh -c 'umask 077 && cat > /var/lib/crystal-forge/host.key'
# Paste the private key, press Enter, then press Ctrl-D.
sudo chown root:root /var/lib/crystal-forge/host.key
```

Do not put the key on a command line with `echo`. The shell would keep the key in its history. A secrets manager that writes the file at activation time also works.

## Apply and Rebuild the Target System

On the target system:

```bash
sudo nixos-rebuild switch
```

This will:
- Install the Crystal Forge agent service
- Start the agent
- Connect to the Crystal Forge server
- Begin sending telemetry (system fingerprints, heartbeats)

## Verify the Agent is Running

On the target system:

```bash
sudo systemctl status crystal-forge-agent
```

You should see:

```
● crystal-forge-agent.service - Crystal Forge Agent
   Active: active (running)
```

Check the logs for successful connection:

```bash
sudo journalctl -u crystal-forge-agent -f
```

The agent signs each report with its private key. If the server rejects a report, the log shows an HTTP error status. Check these causes first:

- The private key does not match the public key registered for the system.
- The `server_host` or `server_port` value is wrong, or the host cannot reach the server.

## What to Expect After Agent Connects

Once the agent connects, Crystal Forge will:

1. **Record System Fingerprint**: Hardware, OS version, network interfaces, security status
2. **Track Heartbeats**: Liveness signals every 600 seconds (10 minutes) by default. The server setting `heartbeat_interval_secs` accepts 15 to 900 seconds, and a per-system value overrides it. The server returns the interval in each heartbeat response.
3. **Monitor State Changes**: Configuration drift, software updates, deployments
4. **Enable Deployments**: The system is now eligible to receive deployment instructions

In the Crystal Forge web UI:

- The **Dashboard** shows the system in the fleet health summary.
- The **Systems** page shows connection status, deployed configuration, and health.
- The **Builds** page shows the build queue and recent build attempts.

## Onboarding Complete!

After the agent sends its first signed report, an Administrator can select **Acknowledge agent setup** in the Setup track. A heartbeat alone does not complete the step. The acknowledgement is saved through the existing setup-progress API.

When all nine setup steps are complete, the Coach shows the setup-complete card and offers **Explore security workflows**. The top-bar **Guide** remains available. Administrators can relaunch Setup from Server Management.

## Related concepts

- [Onboarding guide: first-time server setup prerequisites](onboarding-first-time-setup-prerequisites.md)
- [Guided setup coach, POA&M dashboard notes, and security workflows track](../ui/guided-setup-coach.md)
- [Onboarding troubleshooting](onboarding-troubleshooting.md)
- [Step 5: Register System](onboarding-step-5-system.md)
