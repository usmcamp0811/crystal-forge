---
type: Operator Guide
title: "Step 1: Create Environment"
description: "Walks through creating the first environment in the setup coach: why environments matter, every field of the Add environment form, what the default deployment mode does today, and what gate policies and compliance bundles enforce."
tags:
  - crystal-forge
  - onboarding
  - environments
  - deployment-policy
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T21:00:00-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/onboarding-guide.md at commit 3b23d36f"
    title: "Crystal Forge Onboarding Guide"
  - id: code-1
    resource: "Crystal Forge repository file packages/web-ui/src/components/environments/environment_form_modal.rs at commit 3b23d36f"
    title: Environment form
  - id: code-2
    resource: "Crystal Forge repository file packages/web-ui/src/components/onboarding/setup.rs at commit 3b23d36f"
    title: Setup coach steps
  - id: code-3
    resource: "Crystal Forge repository file modules/nixos/crystal-forge/default.nix at commit 3b23d36f"
    title: Agent deployment strategy option
---

# Step 1: Create Environment

**Environments** in Crystal Forge are logical groupings for your NixOS systems. They represent deployment contexts such as `production`, `staging`, or `development`. An environment is the operational and security boundary for deployment, user access, compliance assignments, and binary caches.

## Why Environments Matter

- **Deployment control:** An environment carries a default deployment mode and the gate policies that a deployment must pass.
- **Cache isolation:** An environment can use its own binary cache.
- **Access control:** A non-Admin user sees only the systems in the environments where the user is a member. An Admin assigns membership directly or through [OIDC group mappings](../security/oidc-role-mapping.md).
- **Compliance tracking:** An environment can require compliance bundles and shows CVE status for its systems.

## Create the environment

1. In the Setup Coach, select **Create environment**. The Environments page opens.
2. Select **Add environment**. A modal opens.
3. Fill in the form (see below) and select **Add environment** in the modal footer.

After you save your first environment, the coach marks **Step 1** complete.

### Form fields

| Field | What it does |
| --- | --- |
| **Name** | A short identifier, for example `production`, `staging`, or `dev`. |
| **Color** | A color tag for the environment in lists and badges. |
| **Description** | Optional text that says what the tier is for. |
| **Cache** | The binary cache assignment for this environment. |
| **Default deployment mode** | `Manual`, `Auto latest`, or `Pinned`. See below. |
| **Policy enforcement** | Gate policies and required compliance bundles. See below. |
| **Production environment** | Marks hosts as production. Destructive actions such as rollback and force-deploy then require a type-to-confirm guard, whatever the environment name is. |
| **Auto-sync flakes** | A stored flag. The Environments page counts environments with auto-sync off. The server code at commit `3b23d36f` does not read the flag to change flake sync. |
| **Require approval before deploy** | A stored flag. The server code at commit `3b23d36f` does not read it to block a deployment. To require approvals, assign an approvals gate policy. |

### Default deployment mode

The form offers `Manual`, `Auto latest`, and `Pinned`. The form states that the server **stores this value as environment metadata today, and that future deployment automation will consume it**. The mode does not change how any system deploys yet.

Each system has its own deployment policy. That policy decides what the server does:

- `manual`: an operator requests each deployment.
- `auto_latest`: the server sets the newest deployable store path for the system's configuration as the target. A newer commit that is pending or failed does not replace it.
- `pinned`: the system stays on a chosen commit.

See [Deployment flow](../deployment/deployment-flow.md) for how a target reaches an agent.

### Deployment strategy is not an environment field

The choice between activating a deployment now (`immediate_persist`) and at the next boot (`boot_only`) belongs to the **agent host**, not to the environment. Set it in the host's NixOS configuration:

```nix
services.crystal-forge.deployment.deployment_strategy = "immediate_persist"; # or "boot_only"
```

`immediate_persist` is the default. It creates a NixOS generation and activates it now. `boot_only` creates the generation and activates it at the next boot.

### Gate policies and compliance bundles

The **Policy enforcement** section applies to every system in the environment. It has two parts:

- **Gate policies:** Deployment policies from the policy library, such as a CVE check, a time window, required approvals, or a canary rollout. Search the library and add the ones you need.
- **Required compliance bundles:** A versioned compliance bundle that you assign for regulated or ATO environments. Only published bundles can be assigned. Create and publish a bundle on the **Compliance** page.

The server evaluates these policies before it delivers a deployment target to an agent. Enforcement depends on the mode of each assignment:

- An **enforced** policy that fails blocks the deployment.
- A **report-only** policy that fails never blocks. The server records the result and the evidence.

Having a policy assigned does not by itself block anything. See [Compliance assignments, overlays, and report-only enforcement](../compliance/assignments-and-report-only-enforcement.md).

You can change gate policies and bundle assignments later by editing the environment.

**Example environment configuration:**

```yaml
Name: production
Description: Customer-facing systems
Default deployment mode: Manual
Gate policies:
  - Require Crystal Forge Agent
Required compliance bundles: []
Production environment: yes
Auto-sync flakes: yes
Require approval before deploy: yes
```

> **Screenshots:** Earlier versions of this guide showed screenshots of a previous six-step coach and an inline "Create Environment" form. The current interface uses a modal and a nine-step coach, so those images are not shown here. See [Guided setup coach](../ui/guided-setup-coach.md) for the current steps.

## Related concepts

- [Onboarding guide: first-time server setup prerequisites](onboarding-first-time-setup-prerequisites.md)
- [Guided setup coach, POA&M dashboard notes, and security workflows track](../ui/guided-setup-coach.md)
- [Onboarding troubleshooting](onboarding-troubleshooting.md)
- [Step 2: Add Flake](onboarding-step-2-flake.md)
