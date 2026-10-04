---
type: Operator Guide
title: "After onboarding and next steps"
description: "Describes where to go after setup completes, how to reopen the coach, common first tasks (build, deploy, CVE results, add a system), and pointers to advanced documentation."
tags:
  - crystal-forge
  - onboarding
  - next-steps
  - operations
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:29-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/onboarding-guide.md at commit 3b23d36f"
    title: "Crystal Forge Onboarding Guide"
---

# After onboarding and next steps

## After Onboarding

Congratulations! You've completed the Crystal Forge initial setup. Here's what to do next.

### Where to Go Next

**Dashboard**: View fleet health, build queue status, recent deployments, and CVE summary
- Route: `/dashboard`

**Systems**: Monitor all managed systems, filter by environment, view deployment status
- Route: `/systems`

**Builds**: See the build queue, completed builds, and CVE scan results
- Route: `/builds`

**Flakes**: Manage tracked repositories, view commit history, trigger evaluations
- Route: `/flakes`

### How to Reopen the Coach

If you dismissed the coach and want to see it again:

1. Open the **user menu** (top-right corner)
2. Click **Server Management**
3. Under "Onboarding," click **Relaunch Setup Coach**

The coach will reappear and show your current progress.

### Common First Tasks

**Trigger a Build Manually:**
1. Go to **Flakes**
2. Click on your flake
3. Click **Evaluate Latest Commit**

**Deploy to a System:**
1. Go to **Systems**
2. Click on the system
3. View available builds
4. Click **Deploy** next to the desired build

**View CVE Scan Results:**
1. Go to **Builds**
2. Click on a completed build
3. Scroll to **CVE Scan Results**
4. Review vulnerabilities and remediation steps

**Add Another System:**
1. Go to **Systems**
2. Click **Add System**
3. Follow the same process as Step 5

> **Status:** The "Where to Go Next" list above gives the Dashboard route as `/dashboard`; `packages/web-ui/src/routes.rs` registers the Dashboard at `/`. Not reconciled in this migration.

## Next Steps

You're now ready to use Crystal Forge! For advanced topics, see:

- **[STIG Compliance Modules](../compliance/stig-modules.md)**: Enable declarative security controls
- **[Deployment Policies](../deployment/deployment-policies.md)**: Understand immediate_persist vs. boot_only
- **[Binary Cache Integration](../caches/cache-push-process.md)**: Configure S3, Attic, or custom caches
- **[OIDC Authentication](../operations/authentication-modes-and-oidc-configuration.md)**: Connect to Keycloak, Authentik, or other identity providers

For questions, issues, or contributions, see [AGENTS.md](../../../AGENTS.md).

**Welcome to Crystal Forge. Happy monitoring!**

## Related concepts

- [Onboarding guide: first-time server setup prerequisites](onboarding-first-time-setup-prerequisites.md)
- [Guided setup coach, POA&M dashboard notes, and security workflows track](../ui/guided-setup-coach.md)
- [Step 6: Deploy Agent](onboarding-step-6-agent-deployment.md)
