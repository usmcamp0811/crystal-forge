---
type: Operator Guide
title: "Onboarding guide: first-time server setup prerequisites"
description: "Introduces the nine-step Setup track and Security Workflows, lists prerequisites, and covers server requirements, PostgreSQL options, and the initial NixOS module configuration needed before the setup coach starts."
tags:
  - crystal-forge
  - onboarding
  - setup
  - nixos
  - postgresql
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:29-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/onboarding-guide.md at commit 3b23d36f"
    title: "Crystal Forge Onboarding Guide"
---

# Crystal Forge Onboarding Guide

Welcome to Crystal Forge! This guide will walk you through your first-time setup using the built-in guided onboarding coach. By the end of this guide, you'll have a fully configured Crystal Forge instance ready to monitor and manage your NixOS fleet.

## Introduction

This guide describes the nine-step Setup track and five Security Workflows walkthroughs in the web interface. Setup completion comes from server-reported persisted resources. Security walkthrough progress is browser-local presentation state only; it does not report security or remediation state.

### What This Guide Covers

- Setting up your Crystal Forge server and database
- Configuring the nine Setup steps via the web UI:
  1. Create an environment
  2. Add a flake
  3. Register a builder
  4. Configure a cache
  5. Register a system
  6. Deploy the agent and acknowledge its first signed report
  7. Create or import a policy
  8. Build a compliance bundle
  9. Track a POA&M
- Using the five Security Workflows walkthroughs available from Guide to every authenticated role

### Prerequisites

Before you begin, you should have:

- **NixOS knowledge**: Familiarity with NixOS modules, flakes, and basic system administration
- **A NixOS server**: Where you'll run the Crystal Forge server, database, and builders
- **Git repository**: Containing your NixOS configurations as a flake
- **Network access**: Ability to reach your managed systems over SSH/HTTP
- **Basic understanding of**:
  - NixOS flakes and how they work
  - Ed25519 cryptographic keys
  - Binary caches (Nix, S3, Attic)

### Overview of the Setup Track

Crystal Forge's web UI includes a non-blocking Coach. Administrators can use the **Setup** track. Operators and Viewers can use **Security workflows**. Every authenticated role can open or reopen the Coach from **Guide** in the top bar.

The server reports Setup completion from saved resources. Opening a page never completes a setup step. Walkthrough progress is stored in this browser and records only which stops were viewed. It does not record whether a CVE, scan, acceptance, compliance control or POA&M is safe, passed, fixed or verified.

## Before You Begin

### Server Requirements

**Minimum recommended specifications for the Crystal Forge server:**

- **CPU**: 4+ cores (for concurrent builds)
- **RAM**: 8GB minimum, 16GB+ recommended
- **Disk**: 50GB+ for PostgreSQL, Nix store, and build artifacts
- **NixOS**: Version 23.11 or later

**Network requirements:**

- Inbound HTTP/HTTPS access on port 3000 (or your configured port)
- Outbound access to your Git repositories
- SSH or HTTP access to managed NixOS systems

### Database Setup

Crystal Forge requires PostgreSQL. You have two options:

**Option 1: Let Crystal Forge manage PostgreSQL** (recommended for single-server setups)

```nix
{
  services.crystal-forge = {
    enable = true;
    local-database = true;  # Crystal Forge will configure PostgreSQL
    
    database = {
      host = "/run/postgresql";  # Unix socket
      user = "crystal_forge";
      name = "crystal_forge";
    };
  };
}
```

**Option 2: Use an existing PostgreSQL instance**

```nix
{
  services.crystal-forge = {
    enable = true;
    
    database = {
      host = "postgres.example.com";
      port = 5432;
      user = "crystal_forge";
      name = "crystal_forge";
      passwordFile = "/run/secrets/db_password";  # agenix, sops-nix, etc.
    };
  };
}
```

### Initial Module Configuration

Add Crystal Forge to your server's NixOS configuration:

```nix
# In your flake.nix or configuration.nix
{
  imports = [
    inputs.crystal-forge.nixosModules.crystal-forge
  ];

  services.crystal-forge = {
    enable = true;
    local-database = true;

    # Server (API + Web UI)
    server = {
      enable = true;
      host = "0.0.0.0";
      port = 3000;
      auth_mode = "local";  # Use local username/password auth
    };

    # Builder (for building server-evaluated derivations)
    build = {
      enable = true;
      max_concurrent_derivations = 2;
      max_jobs = 4;
      cores_per_job = 4;
      systemd_memory_max = "8G";  # Adjust based on your server
    };
  };
}
```

Apply this configuration and rebuild:

```bash
sudo nixos-rebuild switch
```

Verify the server is running:

```bash
sudo systemctl status crystal-forge-server
curl http://localhost:3000/status
```

Navigate to `http://your-server:3000` in a web browser to begin the setup.

## Related concepts

- [Guided setup coach, POA&M dashboard notes, and security workflows track](../ui/guided-setup-coach.md)
- [Step 1: Create Environment](onboarding-step-1-environment.md)
