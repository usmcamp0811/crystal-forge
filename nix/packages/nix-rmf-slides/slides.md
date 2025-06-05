---
title: Secure Mattermost Deployment with Nix
theme: ./themes/slidev-theme-mokkapps
color: dark
---

# Toward Compliant Mattermost Deployment

## Nix-Based Automation for RMF and 800-53 Controls

---
layout: two-cols
---

## Deployment Targets

Mattermost can be securely deployed using one of two primary approaches, each with its own implications for compliance, automation, and runtime hardening:

### 1. **Kubernetes-Based Deployment**

- Uses the official [Mattermost Helm chart](https://github.com/mattermost/mattermost-helm)
- Ideal for cloud-native environments with GitOps workflows
- Compliant deployment requires a hardened `values.yaml`:

  - Secure auth settings
  - TLS configuration via ingress
  - Secrets management (e.g., Vault CSI)
  - Logging and resource constraints

### 2. **Bare Metal / Virtual Machine Deployment**

- Deployed as a systemd service or containerized app
- Suited for air-gapped or traditional environments
- Requires a hardened `config.json` and secure system settings:

  - TLS via NGINX/Traefik
  - Centralized log forwarding
  - File permissions, backup routines, patch management

In both cases:

- You must define a **blessed configuration** that complies with applicable RMF controls
- Configuration must be immutable, version-controlled, and continuously auditable

---

## High-Level Compliance Requirements

1. **Scan the Binary & Dependencies**

   - SBOM with `syft`
   - Vulnerability scan with `grype` or `trivy`

2. **Harden Configuration**

   - Enforce secure auth, logging, encryption, rate limits
   - Disable insecure defaults

3. **Configure Monitoring**

   - Centralized logging, health checks, alerting
   - Patching and CVE tracking

---

## Solution: Nix-Based Secure Deployment Module

Create a NixOS module that:

- Defines **compliant defaults**
- Supports `mode = "kubernetes" | "baremetal"`
- Renders either `values.yaml` or `config.json`
- Handles secrets and TLS

---

## Example Interface

```nix
services.secureMattermost = {
  enable = true;
  mode = "kubernetes";
  domain = "chat.example.com";
  auth.provider = "oidc";
  allowGuestUsers = false;
};
```

---

## Policy Enforcement with Justifications

Use **assertions with conditional justifications**:

```nix
assert cfg.allowGuestUsers == false || cfg.justification.allowGuestUsers != null;
```

```nix
options.services.secureMattermost.allowGuestUsers = mkOption {
  type = types.bool;
  default = false;
};

options.services.secureMattermost.justification.allowGuestUsers = mkOption {
  type = types.nullOr types.str;
  default = null;
};
```

---

## Enforcement Outcomes

| Setting | Justification      | Result  |
| ------- | ------------------ | ------- |
| `true`  | `null`             | ❌ Fail |
| `false` | `null`             | ✅ Pass |
| `true`  | "Scoped exception" | ⚠️ Pass |

---

## Bonus Features

- Vault integration for secrets
- TLS via Traefik/Nginx
- SBOM & vulnerability scan output
- Compliance summary document generation

---

## Summary

✅ Compliant-by-default config
✅ Justifications for deviations
✅ Nix module emits `values.yaml` or `config.json`
✅ Monitoring, patching, and ATO prep baked in

This is how you make secure, compliant deployments **easy**.
