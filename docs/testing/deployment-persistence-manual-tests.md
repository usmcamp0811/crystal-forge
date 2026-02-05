# Manual Testing Procedure: Deployment Persistence

## Overview

This document provides step-by-step procedures for manually testing the deployment persistence fix on a real NixOS system. These tests verify that agent deployments create proper NixOS generations and persist across reboots.

> [!IMPORTANT]
> **Requirements**:
> - Real NixOS system (not a VM - VMs lack internet connectivity in test environment)
> - Crystal Forge server running and accessible
> - Crystal Forge agent installed on test system
> - SSH access to test system
> - Ability to reboot test system

---

## Pre-Deployment Checks

Before running any tests, document the current system state.

### 1. List Current Generations

```bash
sudo nix-env --list-generations --profile /nix/var/nix/profiles/system
```

**Expected Output**:
```
  12   2026-02-01 10:30:45
  13   2026-02-03 14:22:10   (current)
```

**Record**:
- Current generation number: `_____`
- Total number of generations: `_____`

### 2. Check Current System Path

```bash
readlink /run/current-system
```

**Expected Output**:
```
/nix/store/abc123...-nixos-system-hostname-24.11
```

**Record**:
- Current system store path: `_____________________________`

### 3. Check Bootloader Entries

```bash
ls -la /boot/loader/entries/
```

**Expected Output**:
```
nixos-generation-12.conf
nixos-generation-13.conf
```

**Record**:
- Number of bootloader entries: `_____`

---

## Test 1: Immediate Persist Strategy (Default)

This test verifies that deployments with the `immediate_persist` strategy create generations and activate immediately.

### Configuration

Ensure the agent configuration uses `immediate_persist` strategy (this is the default):

```toml
[deployment]
strategy = "immediate_persist"
```

Or in NixOS configuration:

```nix
services.crystal-forge.deployment.deployment_strategy = "immediate_persist";
```

### Test Steps

#### Step 1: Trigger Deployment

Trigger a deployment through Crystal Forge (method depends on your setup):

**Option A**: Via server API
```bash
curl -X POST http://server:3000/api/deploy \
  -H "Content-Type: application/json" \
  -d '{"system": "test-system", "target": "latest"}'
```

**Option B**: Via manual agent trigger
```bash
sudo systemctl restart crystal-forge-agent
```

**Option C**: Via configuration change
- Push a configuration change to your flake
- Wait for Crystal Forge to detect and deploy

**Record**:
- Deployment method used: `_____________________`
- Deployment timestamp: `_____________________`

#### Step 2: Monitor Deployment

Watch the agent logs:

```bash
sudo journalctl -u crystal-forge-agent -f
```

**Look for**:
- `Creating new NixOS generation...`
- `✅ Generation created successfully`
- `✅ Generation verified`
- `Using immediate_persist strategy: activating now`
- `✅ Configuration activated successfully`

**Record**:
- Did you see all expected log messages? `[ ] Yes [ ] No`
- Any errors? `_____________________`

#### Step 3: Verify Generation Created

```bash
sudo nix-env --list-generations --profile /nix/var/nix/profiles/system
```

**Expected**: New generation should appear (e.g., generation 14)

**Record**:
- New generation number: `_____`
- Generation timestamp matches deployment? `[ ] Yes [ ] No`

#### Step 4: Verify Generation Points to Deployed Path

```bash
readlink /nix/var/nix/profiles/system
```

**Expected**: Should point to the newly deployed store path

**Record**:
- New system store path: `_____________________________`
- Different from pre-deployment path? `[ ] Yes [ ] No`

#### Step 5: Verify Current System Updated

```bash
readlink /run/current-system
```

**Expected**: Should match the new generation path (immediate activation)

**Record**:
- Current system matches new generation? `[ ] Yes [ ] No`

#### Step 6: Verify Bootloader Updated

```bash
ls -la /boot/loader/entries/
```

**Expected**: New bootloader entry should exist

**Record**:
- New bootloader entry exists? `[ ] Yes [ ] No`
- Entry filename: `_____________________`

#### Step 7: Reboot and Verify Persistence

```bash
sudo reboot
```

After reboot:

```bash
# Check current system
readlink /run/current-system

# Check generation list
sudo nix-env --list-generations --profile /nix/var/nix/profiles/system

# Verify it's marked as current
```

**Expected**: System should boot into the new generation

**Record**:
- System booted into new generation? `[ ] Yes [ ] No`
- Configuration persisted across reboot? `[ ] Yes [ ] No`

### Test 1 Results

- [ ] ✅ Generation created
- [ ] ✅ Generation verified
- [ ] ✅ System activated immediately
- [ ] ✅ Bootloader updated
- [ ] ✅ Configuration persisted across reboot

**Overall Result**: `[ ] PASS [ ] FAIL`

**Notes**: 
```
_____________________________________________________
_____________________________________________________
_____________________________________________________
```

---

## Test 2: Boot Only Strategy

This test verifies that deployments with the `boot_only` strategy create generations but only activate on next boot.

### Configuration

Update the agent configuration to use `boot_only` strategy:

```toml
[deployment]
strategy = "boot_only"
```

Or in NixOS configuration:

```nix
services.crystal-forge.deployment.deployment_strategy = "boot_only";
```

**Important**: Restart the agent after changing configuration:

```bash
sudo systemctl restart crystal-forge-agent
```

### Test Steps

#### Step 1: Record Pre-Deployment State

```bash
# Current generation
sudo nix-env --list-generations --profile /nix/var/nix/profiles/system | tail -1

# Current system
readlink /run/current-system
```

**Record**:
- Current generation before deployment: `_____`
- Current system path: `_____________________________`

#### Step 2: Trigger Deployment

Use the same method as Test 1 to trigger a deployment.

**Record**:
- Deployment method: `_____________________`
- Deployment timestamp: `_____________________`

#### Step 3: Monitor Deployment

Watch the agent logs:

```bash
sudo journalctl -u crystal-forge-agent -f
```

**Look for**:
- `Creating new NixOS generation...`
- `✅ Generation created successfully`
- `✅ Generation verified`
- `Using boot_only strategy: will activate on next boot`
- `✅ Configuration activated successfully`

**Record**:
- Did you see all expected log messages? `[ ] Yes [ ] No`
- Strategy message says "boot_only"? `[ ] Yes [ ] No`

#### Step 4: Verify Generation Created

```bash
sudo nix-env --list-generations --profile /nix/var/nix/profiles/system
```

**Expected**: New generation should appear

**Record**:
- New generation number: `_____`
- Generation created? `[ ] Yes [ ] No`

#### Step 5: Verify Current System NOT Updated

```bash
readlink /run/current-system
```

**Expected**: Should still point to the OLD system path (not activated yet)

**Record**:
- Current system still points to old path? `[ ] Yes [ ] No`
- This is correct for boot_only strategy

#### Step 6: Verify Bootloader Updated

```bash
ls -la /boot/loader/entries/
```

**Expected**: New bootloader entry should exist (even though not activated yet)

**Record**:
- New bootloader entry exists? `[ ] Yes [ ] No`

#### Step 7: Reboot and Verify Activation

```bash
sudo reboot
```

After reboot:

```bash
# Check current system
readlink /run/current-system

# Check which generation is current
sudo nix-env --list-generations --profile /nix/var/nix/profiles/system

# Verify the new generation is now active
```

**Expected**: System should now be running the new generation

**Record**:
- System booted into new generation? `[ ] Yes [ ] No`
- Current system matches new generation? `[ ] Yes [ ] No`

### Test 2 Results

- [ ] ✅ Generation created
- [ ] ✅ Generation verified
- [ ] ✅ System NOT activated immediately (stayed on old system)
- [ ] ✅ Bootloader updated
- [ ] ✅ System activated on next boot
- [ ] ✅ Configuration persisted across reboot

**Overall Result**: `[ ] PASS [ ] FAIL`

**Notes**:
```
_____________________________________________________
_____________________________________________________
_____________________________________________________
```

---

## Test 3: Multiple Deployments

This test verifies that multiple consecutive deployments work correctly and maintain generation history.

### Test Steps

#### Step 1: Perform Three Deployments

With `immediate_persist` strategy, trigger three deployments in sequence:

1. **Deployment A**: Trigger deployment, wait for completion
2. **Deployment B**: Trigger deployment, wait for completion
3. **Deployment C**: Trigger deployment, wait for completion

**Record**:
- Deployment A timestamp: `_____________________`
- Deployment B timestamp: `_____________________`
- Deployment C timestamp: `_____________________`

#### Step 2: Verify Generation History

```bash
sudo nix-env --list-generations --profile /nix/var/nix/profiles/system
```

**Expected**: Should show all three new generations

**Record**:
- Generation A number: `_____`
- Generation B number: `_____`
- Generation C number: `_____`
- All three generations present? `[ ] Yes [ ] No`

#### Step 3: Verify Current System

```bash
readlink /run/current-system
```

**Expected**: Should point to the latest deployment (C)

**Record**:
- Current system is deployment C? `[ ] Yes [ ] No`

#### Step 4: Test Rollback to Previous Generation

```bash
# Switch to generation B
sudo /nix/var/nix/profiles/system-<B-number>-link/bin/switch-to-configuration switch

# Verify
readlink /run/current-system
```

**Expected**: System should now be running generation B

**Record**:
- Rollback successful? `[ ] Yes [ ] No`
- Current system is generation B? `[ ] Yes [ ] No`

#### Step 5: Reboot and Verify

```bash
sudo reboot
```

After reboot:

```bash
readlink /run/current-system
```

**Expected**: System should still be on generation B (rollback persisted)

**Record**:
- System still on generation B after reboot? `[ ] Yes [ ] No`

### Test 3 Results

- [ ] ✅ Multiple deployments created multiple generations
- [ ] ✅ Each generation is distinct
- [ ] ✅ Latest deployment became current
- [ ] ✅ Rollback to previous generation works
- [ ] ✅ Rollback persists across reboot

**Overall Result**: `[ ] PASS [ ] FAIL`

**Notes**:
```
_____________________________________________________
_____________________________________________________
_____________________________________________________
```

---

## Test 4: Configuration Validation

This test verifies that the configuration is correctly generated and parsed.

### Test Steps

#### Step 1: Check Generated Config

```bash
sudo cat /var/lib/crystal-forge-agent/config.toml | grep -A 5 "\[deployment\]"
```

**Expected Output** (with immediate_persist):
```toml
[deployment]
max_deployment_age_minutes = 30
dry_run_first = true
deployment_timeout_minutes = 60
deployment_poll_interval = "15m"
require_sigs = true
strategy = "immediate_persist"
```

**Record**:
- Strategy field present? `[ ] Yes [ ] No`
- Strategy value correct? `[ ] Yes [ ] No`

#### Step 2: Change Strategy and Verify

Update NixOS configuration to use `boot_only`:

```nix
services.crystal-forge.deployment.deployment_strategy = "boot_only";
```

Rebuild and switch:

```bash
sudo nixos-rebuild switch
```

Check config again:

```bash
sudo cat /var/lib/crystal-forge-agent/config.toml | grep -A 5 "\[deployment\]"
```

**Expected**: Strategy should now be `"boot_only"`

**Record**:
- Strategy updated to boot_only? `[ ] Yes [ ] No`

### Test 4 Results

- [ ] ✅ Configuration file includes strategy field
- [ ] ✅ Default strategy is immediate_persist
- [ ] ✅ Strategy can be changed via NixOS configuration
- [ ] ✅ Configuration changes are applied correctly

**Overall Result**: `[ ] PASS [ ] FAIL`

**Notes**:
```
_____________________________________________________
_____________________________________________________
_____________________________________________________
```

---

## Troubleshooting

### Issue: Generation Not Created

**Symptoms**:
- No new generation appears in `nix-env --list-generations`
- Logs show "Failed to create generation"

**Possible Causes**:
1. Insufficient permissions (agent not running as root)
2. Store path not available
3. Nix store corruption

**Debug Steps**:
```bash
# Check agent is running as root
ps aux | grep crystal-forge-agent

# Check store path exists
ls -la /nix/store/<store-path>

# Check nix-env works manually
sudo nix-env --profile /nix/var/nix/profiles/system --set /nix/store/<store-path>
```

### Issue: Activation Fails

**Symptoms**:
- Generation created but system not activated
- Logs show "systemd-run failed"

**Possible Causes**:
1. systemd-run not available
2. switch-to-configuration script missing
3. Permissions issue

**Debug Steps**:
```bash
# Check systemd-run works
systemd-run --version

# Check switch-to-configuration exists
ls -la /nix/store/<store-path>/bin/switch-to-configuration

# Try manual activation
sudo /nix/store/<store-path>/bin/switch-to-configuration switch
```

### Issue: Deployment Doesn't Persist

**Symptoms**:
- Deployment works but reverts after reboot
- Bootloader doesn't show new generation

**Possible Causes**:
1. Generation not created (only activated)
2. Bootloader not updated
3. Wrong generation set as default

**Debug Steps**:
```bash
# Check generations
sudo nix-env --list-generations --profile /nix/var/nix/profiles/system

# Check bootloader entries
ls -la /boot/loader/entries/

# Check default boot entry
cat /boot/loader/loader.conf
```

---

## Test Summary

### Overall Results

| Test | Result | Notes |
|------|--------|-------|
| Test 1: Immediate Persist | [ ] PASS [ ] FAIL | |
| Test 2: Boot Only | [ ] PASS [ ] FAIL | |
| Test 3: Multiple Deployments | [ ] PASS [ ] FAIL | |
| Test 4: Configuration Validation | [ ] PASS [ ] FAIL | |

### Sign-Off

**Tester**: `_____________________`  
**Date**: `_____________________`  
**System**: `_____________________`  
**NixOS Version**: `_____________________`  
**Crystal Forge Version**: `_____________________`

**Overall Assessment**: `[ ] All tests passed [ ] Some tests failed [ ] Major issues found`

**Additional Notes**:
```
_____________________________________________________
_____________________________________________________
_____________________________________________________
_____________________________________________________
_____________________________________________________
```

---

## Appendix: Quick Reference Commands

### Check Current State

```bash
# List generations
sudo nix-env --list-generations --profile /nix/var/nix/profiles/system

# Current system
readlink /run/current-system

# Bootloader entries
ls -la /boot/loader/entries/

# Agent logs
sudo journalctl -u crystal-forge-agent -f

# Agent config
sudo cat /var/lib/crystal-forge-agent/config.toml
```

### Manual Operations

```bash
# Create generation manually
sudo nix-env --profile /nix/var/nix/profiles/system --set /nix/store/<path>

# Activate configuration
sudo /nix/store/<path>/bin/switch-to-configuration switch

# Rollback to previous generation
sudo nix-env --rollback --profile /nix/var/nix/profiles/system
sudo /nix/var/nix/profiles/system/bin/switch-to-configuration switch

# Switch to specific generation
sudo /nix/var/nix/profiles/system-<number>-link/bin/switch-to-configuration switch
```

### Restart Services

```bash
# Restart agent
sudo systemctl restart crystal-forge-agent

# Check agent status
sudo systemctl status crystal-forge-agent

# Rebuild NixOS configuration
sudo nixos-rebuild switch
```
