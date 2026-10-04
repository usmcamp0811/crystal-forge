---
type: Operator Guide
title: "Onboarding troubleshooting"
description: "Troubleshooting guide for first-time setup: coach panel not appearing, steps not completing, agent not connecting, builder not activating, and common configuration mistakes."
tags:
  - crystal-forge
  - onboarding
  - troubleshooting
  - agent
  - builder
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:29-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/onboarding-guide.md at commit 3b23d36f"
    title: "Crystal Forge Onboarding Guide"
---

# Troubleshooting

## Coach Panel Not Appearing

**Symptom**: After logging in as admin, the coach panel doesn't show.

**Possible causes:**
- You're not logged in as an admin (only admins see the coach)
- The coach was previously dismissed and persisted that state
- Browser localStorage is disabled

**Solutions:**
1. Verify you're logged in as an admin (check user menu)
2. Go to **Server Management** → **Relaunch Setup Coach**
3. Clear browser localStorage: `localStorage.removeItem('cf.coach.dismissed')`
4. Refresh the page

## Steps Not Marking Complete

**Symptom**: You created an environment/flake/builder/cache/system, but the coach step is still showing as incomplete.

**Possible causes:**
- The backend hasn't refreshed progress yet (the coach polls every 8 seconds)
- The entity was created but doesn't meet completion criteria (e.g., cache not assigned to any environment)

**Solutions:**
1. Wait 10 seconds and check again (the coach auto-refreshes)
2. Click the **Refresh** button in the coach panel footer
3. Verify the entity was created successfully (check the relevant page: Environments, Flakes, etc.)
4. For the cache step: ensure at least one cache destination exists (even global/unassigned counts)

## Agent Not Connecting

**Symptom**: You enabled the agent on a target system, but it's not showing as connected in the web UI.

**Possible causes:**
- Agent service isn't running
- Network connectivity issue (firewall, wrong server host/port)
- Private key mismatch (public key in Crystal Forge doesn't match private key on system)
- Ed25519 signature verification failed

**Solutions:**

1. **Verify the agent service is running:**
   ```bash
   sudo systemctl status crystal-forge-agent
   ```
   If not running, check logs:
   ```bash
   sudo journalctl -u crystal-forge-agent -n 50
   ```

2. **Check network connectivity:**
   ```bash
   curl http://crystal-forge.example.com:3000/status
   ```
   If this fails, check firewall rules and DNS.

3. **Verify key pair match:**
   - Public key in Crystal Forge web UI (Systems page)
   - Private key on target system (`/var/lib/crystal-forge/host.key`)
   - Regenerate the key pair if needed and update both sides

4. **Check server logs for signature verification errors:**
   ```bash
   sudo journalctl -u crystal-forge-server -f
   ```
   Look for Ed25519 signature failures.

## Builder Not Activating

**Symptom**: You created a builder in the web UI and configured the NixOS module, but builds aren't running.

**Possible causes:**
- Builder service isn't running
- Private key mismatch
- Resource limits too restrictive (builder can't claim any work)

**Solutions:**

1. **Verify builder service:**
   ```bash
   sudo systemctl status crystal-forge-builder
   ```

2. **Check builder logs:**
   ```bash
   sudo journalctl -u crystal-forge-builder -f
   ```

3. **Verify resource configuration matches:**
   - Web UI: max_concurrent_derivations, max CPU, max memory
   - NixOS config: should match or be compatible

4. **Check for work in the build queue:**
   - Go to **Builds** in the web UI
   - If the queue is empty, trigger an evaluation from **Flakes**

## Common Configuration Mistakes

**Private key file permissions:**

If the agent or builder can't read the private key:

```bash
sudo chown crystal-forge:crystal-forge /var/lib/crystal-forge/*.key
sudo chmod 600 /var/lib/crystal-forge/*.key
```

**Wrong server host/port in agent config:**

Verify the agent can reach the server:

```bash
curl http://<server_host>:<server_port>/status
```

**Flake repository not accessible:**

Verify the Crystal Forge server has SSH/HTTPS access to the Git repository:

```bash
# On the Crystal Forge server
sudo -u crystal-forge git clone <repo_url>
```

If this fails, check SSH keys (for SSH URLs) or network access (for HTTPS).

**STIG control mismatch:**

If you configured required policies in an environment, but a system's flake doesn't enable those controls, Crystal Forge will block deployment. Check:

- Environment required policies (Environments page)
- System's flake configuration (does it enable those STIG modules?)

> **Status:** This guide says that only administrators see the coach and that dismissal persists as `cf.coach.dismissed` in browser localStorage. The Guided Setup Coach section says every authenticated role can reopen the Coach from **Guide**, and `packages/web-ui/src/components/onboarding/state.rs` stores presentation state under the key `cf.coach.ui.v2`. Not reconciled in this migration.

## Related concepts

- [Onboarding guide: first-time server setup prerequisites](onboarding-first-time-setup-prerequisites.md)
- [Guided setup coach, POA&M dashboard notes, and security workflows track](../ui/guided-setup-coach.md)
- [Step 6: Deploy Agent](onboarding-step-6-agent-deployment.md)
