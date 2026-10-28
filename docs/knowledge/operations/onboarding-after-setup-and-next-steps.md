---
type: Operator Guide
title: "After onboarding and next steps"
description: "Describes where to go after setup completes, how to reopen the coach, common first tasks (sync and re-evaluate, deploy, CVE results, add a system), and pointers to advanced documentation."
tags:
  - crystal-forge
  - onboarding
  - next-steps
  - operations
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T21:00:00-05:00
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
- Route: `/`

**Systems**: Monitor all managed systems, filter by environment, view deployment status
- Route: `/systems`

**Environments**: Review environment settings, gate policies, and compliance bundle assignments
- Route: `/environments`

**Flakes**: Manage tracked repositories, view commit history, and sync
- Route: `/flakes`

**Evaluations**: See the evaluation queue and history for each commit
- Route: `/evaluations`

**Builds**: See the build queue and recent build attempts
- Route: `/builds`

**CVEs and Scanning**: Review vulnerabilities by CVE, and the scan queue and results
- Routes: `/cves`, `/scanning`

**POA&Ms and Compliance**: Plan remediation, and manage compliance bundles and assignments
- Routes: `/poams`, `/compliance`

### How to Reopen the Coach

If you dismissed the coach and want to see it again:

1. Open the **user menu** (top-right corner)
2. Click **Server Management**
3. Under "Onboarding," click **Relaunch Setup Coach**

The coach will reappear and show your current progress.

### Common First Tasks

**Pick up new commits:**
1. Go to **Flakes**
2. Select **Sync** on your flake, or **Sync all**
3. The server evaluates each new commit, one at a time. Follow progress on **Evaluations**.

**Run an evaluation again** (Admin only):
1. Go to **Evaluations**
2. Select **Re-evaluate** on the commit

**Deploy to a System:**
1. Go to **Systems**
2. Select the system
3. Open the **Deploy** tab and choose a commit
4. Confirm the deployment

A commit is deployable only when its build is published to the binary cache. If it is not, the server queues the missing build first. See [Deployment flow](../deployment/deployment-flow.md).

**View CVE Scan Results:**
1. Go to **CVEs** for fleet-wide results, or open a system and its CVE view
2. Review the vulnerabilities and their severity
3. Go to **Scanning** to see scans that are queued or running

**Add Another System:**
1. Go to **Systems**
2. Select **Add system**
3. Follow the same process as Step 5

## Next Steps

You're now ready to use Crystal Forge! For advanced topics, see:

- **[STIG Compliance Modules](../compliance/stig-modules.md)**: Enable declarative security controls
- **[Deployment Policies](../deployment/deployment-policies.md)**: Understand how policies allow, warn, block, or hold a deployment. The `immediate_persist` and `boot_only` agent strategies are in [Deployment flow](../deployment/deployment-flow.md).
- **[Binary Cache Integration](../caches/cache-push-process.md)**: Configure S3, Attic, or custom caches
- **[OIDC Authentication](../operations/authentication-modes-and-oidc-configuration.md)**: Connect to Keycloak, Authentik, or other identity providers

For questions, issues, or contributions, see [AGENTS.md](../../../AGENTS.md).

**Welcome to Crystal Forge. Happy monitoring!**

## Related concepts

- [Onboarding guide: first-time server setup prerequisites](onboarding-first-time-setup-prerequisites.md)
- [Guided setup coach, POA&M dashboard notes, and security workflows track](../ui/guided-setup-coach.md)
- [Step 6: Deploy Agent](onboarding-step-6-agent-deployment.md)
