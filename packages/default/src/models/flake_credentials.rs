use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FlakeCredentialAuthType {
    Pat,
    SshKey,
    UsernamePassword,
}

impl std::str::FromStr for FlakeCredentialAuthType {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pat" => Ok(Self::Pat),
            "ssh_key" => Ok(Self::SshKey),
            "username_password" => Ok(Self::UsernamePassword),
            _ => Err(anyhow::anyhow!(
                "invalid flake credential auth type: {value}"
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct FlakeCredential {
    pub id: i32,
    pub flake_id: i32,
    pub auth_type: String,
    pub username: Option<String>,
    pub secret_encrypted: Option<String>,
    pub ssh_username: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateFlakeCredential {
    pub auth_type: String,
    pub username: Option<String>,
    pub secret: Option<String>,
    pub ssh_username: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateFlakeCredential {
    pub auth_type: Option<String>,
    pub username: Option<String>,
    pub secret: Option<String>,
    pub ssh_username: Option<String>,
}

impl CreateFlakeCredential {
    pub fn validate(&self) -> Result<(), String> {
        validate_flake_credential_shape(
            &self.auth_type,
            self.username.as_deref(),
            self.secret.as_deref(),
            self.ssh_username.as_deref(),
        )
    }
}

impl UpdateFlakeCredential {
    pub fn validate_against(&self, current: &FlakeCredential) -> Result<(), String> {
        let auth_type = self
            .auth_type
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&current.auth_type);
        let username = self.username.as_deref().or(current.username.as_deref());
        let secret = self
            .secret
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or(current.secret_encrypted.as_deref());
        let ssh_username = self
            .ssh_username
            .as_deref()
            .or(current.ssh_username.as_deref());
        validate_flake_credential_shape(auth_type, username, secret, ssh_username)
    }
}

fn validate_flake_credential_shape(
    auth_type: &str,
    username: Option<&str>,
    secret: Option<&str>,
    ssh_username: Option<&str>,
) -> Result<(), String> {
    let parsed = auth_type
        .parse::<FlakeCredentialAuthType>()
        .map_err(|err| err.to_string())?;

    match parsed {
        FlakeCredentialAuthType::Pat => {
            if secret.is_none_or(|value| value.trim().is_empty()) {
                return Err("PAT credentials require a token secret".to_string());
            }
        }
        FlakeCredentialAuthType::SshKey => {
            if secret.is_none_or(|value| value.trim().is_empty()) {
                return Err("SSH credentials require a private key".to_string());
            }
        }
        FlakeCredentialAuthType::UsernamePassword => {
            if username.is_none_or(|value| value.trim().is_empty()) {
                return Err("Username/password credentials require a username".to_string());
            }
            if secret.is_none_or(|value| value.trim().is_empty()) {
                return Err("Username/password credentials require a password secret".to_string());
            }
        }
    }

    if ssh_username.is_some_and(|value| value.trim().is_empty()) {
        return Err("SSH username must not be empty when provided".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{FlakeCredential, UpdateFlakeCredential};
    use chrono::Utc;

    #[test]
    fn validate_against_treats_blank_secret_as_keep_existing() {
        let current = FlakeCredential {
            id: 1,
            flake_id: 1,
            auth_type: "pat".to_string(),
            username: Some("x-access-token".to_string()),
            secret_encrypted: Some("existing-secret".to_string()),
            ssh_username: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let update = UpdateFlakeCredential {
            auth_type: Some("pat".to_string()),
            username: Some("x-access-token".to_string()),
            secret: Some("   ".to_string()),
            ssh_username: None,
        };

        assert!(update.validate_against(&current).is_ok());
    }
}
