# Crystal Forge STIG Module System

## Overview

The STIG module system provides declarative compliance configuration for NixOS systems through Crystal Forge. It enables fine-grained control over individual STIG (Security Technical Implementation Guide) controls with mandatory justification when controls are disabled.

## Architecture

### Core Components

The system consists of three layers:

1. **`mkStigModule` Function** — A factory function that generates NixOS modules for individual STIG controls
2. **Tracking Structure** — Options that record which controls are active and inactive
3. **Control Modules** — Individual modules (like `banner`) that use `mkStigModule` to declare specific controls

### File Structure

```
modules/nixos/stig/
├── default.nix           # Defines tracking options and imports all controls
├── banner/
│   └── default.nix       # Uses mkStigModule to define banner control
└── account_expiry/
    └── default.nix       # Uses mkStigModule to define account_expiry control
```

## How It Works

### 1. Import All Controls into Top Level STIG Module

`modules/nixos/stig/default.nix` `imports` all controls:

```nix
{lib, ...}: {

  imports = [
    ./banner
    ./account_expiry
    # Add new controls here
  ];
}
```

### 2. Declare Individual Controls

Each control module (e.g., `banner/default.nix`) uses `mkStigModule` to define options and behavior:

```nix
{lib, config, ...}:
with lib;
with lib.crystal-forge;
mkStigModule {
  inherit config;
  name = "banner";
  srgList = ["SRG-OS-000023-GPOS-00006"];
  cciList = ["CCI-000048"];
  stigConfig = {
    services.openssh.banner = "...";
    services.getty.helpLine = "...";
  };
}
```

### 3. mkStigModule Function

The function generates a NixOS module with:

**Options:**

- `crystal-forge.stig.${name}.enable` — Boolean toggle (defaults to true)
- `crystal-forge.stig.${name}.justification` — List of strings explaining why disabled

**Configuration:**

- When enabled: Applies `stigConfig` with `mkForce` to prevent overrides
- Populates `crystal-forge.stig.active.${name}` with SRG, CCI, and applied config
- Populates `crystal-forge.stig.inactive.${name}` with SRG, CCI, justification, and unapplied config
- Enforces assertion: if disabled, justification is required

**Example behavior:**

```nix
# Enabled (default)
crystal-forge.stig.banner.enable = true;
# Result: config.crystal-forge.stig.active.banner populated
#         SSH and Getty banners applied

# Disabled with justification
crystal-forge.stig.account_expiry = {
  enable = false;
  justification = ["Not applicable in dev environment"];
};
# Result: config.crystal-forge.stig.inactive.account_expiry populated
#         Account expiry config NOT applied
```

## Configuration

### Per-Control Configuration

Each STIG control is independently controllable:

```nix
# Enable a control (default)
crystal-forge.stig.banner.enable = true;

# Disable with justification
crystal-forge.stig.account_expiry = {
  enable = false;
  justification = [
    "Development systems don't require account expiry"
    "Reviewed and approved by security team"
  ];
};
```

### No Global Enable

Unlike some compliance systems, there is **no global `crystal-forge.stig.enable`**. Each control defaults to enabled, providing "secure by default" behavior while allowing explicit opt-out with justification.

## Audit and Reporting

The system automatically tracks compliance state in read-only attributes:

### Active Controls

```nix
config.crystal-forge.stig.active.banner = {
  srg = ["SRG-OS-000023-GPOS-00006"];
  cci = ["CCI-000048"];
  config = { /* the NixOS config that was applied */ };
};
```

### Inactive Controls

```nix
config.crystal-forge.stig.inactive.account_expiry = {
  srg = ["SRG-OS-000002-GPOS-00002"];
  cci = ["CCI-000016"];
  justification = ["Not applicable in dev environment"];
  config = { /* the config that was NOT applied */ };
};
```

This structure enables:

- Automated compliance reports (what's active vs inactive)
- Justification tracking (why controls are disabled)
- Configuration versioning (what each control enforces)

## Adding New STIG Controls

1. Create a new directory: `modules/nixos/stig/control_name/default.nix`

2. Implement the control using `mkStigModule`:

```nix
{lib, config, ...}:
with lib;
with lib.crystal-forge;
mkStigModule {
  inherit config;
  name = "control_name";
  srgList = ["SRG-xxx"];
  cciList = ["CCI-xxx"];
  stigConfig = {
    # NixOS configuration to apply when enabled
  };
}
```

3. Import it in `modules/nixos/stig/default.nix`:

```nix
imports = [
  ./banner
  ./account_expiry
  ./control_name  # Add here
];
```

## Key Design Principles

- **Secure by Default** — Controls are enabled unless explicitly disabled with justification
- **Immutable When Enabled** — `mkForce` prevents accidental configuration conflicts
- **Mandatory Justification** — Disabling a control requires explicit reasons
- **Fine-Grained Control** — Each control is independently configurable
- **Audit Trail** — All active and inactive controls are tracked with metadata
- **Compliance Automation** — Tracking structure enables automated reporting and policy enforcement

## Using Downstream

### Import the STIG Module

In your downstream flake's `flake.nix`, add `crystal-forge.nixosModules.stig` to your `systems.modules.nixos`:

```nix
systems.modules.nixos = with inputs; [
  nixtheplanet.nixosModules.macos-ventura
  home-manager.nixosModules.home-manager
  crystal-forge.nixosModules.crystal-forge
  crystal-forge.nixosModules.stig  # Import all STIG controls
];
```

### Configure Controls

In your system configuration, enable or disable individual controls:

```nix
# Enable a control (default behavior)
crystal-forge.stig.banner.enable = true;

# Disable with justification
crystal-forge.stig.account_expiry = {
  enable = false;
  justification = ["Not applicable in development environment"];
};

# Multiple controls in one block
crystal-forge.stig = {
  banner = {
    enable = true;
  };
  account_expiry = {
    enable = false;
    justification = ["Development systems don't require expiry"];
  };
};
```

Once imported, all STIG controls from Crystal Forge are available for configuration. Each can be individually enabled or disabled with appropriate justification.
