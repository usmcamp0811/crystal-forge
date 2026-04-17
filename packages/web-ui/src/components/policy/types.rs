//! Shared types for policy components.

use uuid::Uuid;

/// Policy definition format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyFormat {
    Toml,
    Json,
}

/// A deployment policy definition.
#[derive(Clone, Debug, PartialEq)]
pub struct PolicyDefinition {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub format: PolicyFormat,
    pub body: String,
    /// The policy type (e.g., "require_cf_agent", "require_crystal_forge_agent", "require_packages", "custom_check").
    /// Optional for backward compatibility with mock/TOML policies that don't have this field.
    pub policy_type: Option<String>,
}

/// Sample TOML policy body used as a default in the editor.
pub const POLICY_TOML_SAMPLE: &str = r#"[[policy]]
type = "require_crystal_forge_agent"
strict = true

[[policy]]
type = "require_packages"
packages = ["git", "vim"]
strict = false

[[policy]]
type = "custom_check"
expression = "(cfg.config.services.openssh.enable or false)"
description = "SSH must be enabled"
field_name = "sshEnabled"
strict = true
"#;
