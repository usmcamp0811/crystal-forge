use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose;
use chrono::{DateTime, Utc};
use ed25519_dalek::VerifyingKey;
use serde::ser::StdError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sqlx::{Database, Decode, Encode, FromRow, Type};
use std::option::Option;
use uuid::Uuid;

/// A validated Ed25519 public key wrapper
#[derive(Debug, Clone, PartialEq)]
pub struct PublicKey(VerifyingKey);

impl PublicKey {
    /// Create a new PublicKey from a base64 string
    pub fn from_base64(base64_str: &str, hostname: &str) -> Result<Self> {
        if base64_str.is_empty() {
            return Err(anyhow::anyhow!(
                "Public key cannot be empty for system {}",
                hostname
            ));
        }

        let key_bytes = general_purpose::STANDARD.decode(base64_str).map_err(|e| {
            anyhow::anyhow!(
                "Failed to decode base64 public key for system {}: {e}",
                hostname
            )
        })?;

        let key_array: [u8; 32] = key_bytes.as_slice().try_into().map_err(|_| {
            anyhow::anyhow!(
                "Public key for system {} must be exactly 32 bytes, got {}",
                hostname,
                key_bytes.len()
            )
        })?;

        let verifying_key = VerifyingKey::from_bytes(&key_array).map_err(|e| {
            anyhow::anyhow!("Invalid ed25519 public key for system {}: {e}", hostname)
        })?;

        Ok(PublicKey(verifying_key))
    }

    /// Create a PublicKey from a VerifyingKey
    pub fn from_verifying_key(key: VerifyingKey) -> Self {
        PublicKey(key)
    }

    /// Get the underlying VerifyingKey
    pub fn verifying_key(&self) -> &VerifyingKey {
        &self.0
    }

    /// Convert to base64 string for storage
    pub fn to_base64(&self) -> String {
        general_purpose::STANDARD.encode(self.0.to_bytes())
    }

    /// Get the raw bytes
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }
}

// Custom serialization for JSON/serde
impl Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_base64())
    }
}

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let base64_str = String::deserialize(deserializer)?;
        // For deserialization, we don't have hostname context, so use "unknown"
        PublicKey::from_base64(&base64_str, "unknown").map_err(serde::de::Error::custom)
    }
}

// SQLx database type implementations
impl<DB: Database> Type<DB> for PublicKey
where
    String: Type<DB>,
{
    fn type_info() -> <DB as Database>::TypeInfo {
        <String as Type<DB>>::type_info()
    }
}

impl<'r, DB: Database> Decode<'r, DB> for PublicKey
where
    String: Decode<'r, DB>,
{
    fn decode(
        value: <DB as Database>::ValueRef<'r>,
    ) -> Result<PublicKey, Box<dyn StdError + Send + Sync>> {
        let base64_str = <String as Decode<'r, DB>>::decode(value)?;
        PublicKey::from_base64(&base64_str, "database").map_err(|e| e.into())
    }
}

impl<'q, DB: Database> Encode<'q, DB> for PublicKey
where
    String: Encode<'q, DB>,
{
    fn encode_by_ref(
        &self,
        buf: &mut <DB as Database>::ArgumentBuffer<'q>,
    ) -> Result<sqlx::encode::IsNull, Box<(dyn StdError + Send + Sync + 'static)>> {
        Ok(self.to_base64().encode_by_ref(buf)?)
    }
}

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct System {
    pub id: Uuid,
    pub hostname: String,
    pub environment_id: Option<Uuid>,
    pub is_active: bool,
    pub public_key: PublicKey,
    pub flake_id: Option<i32>,
    pub derivation: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl System {
    /// Create a new System with validated public key from base64 string
    pub fn new(
        hostname: String,
        environment_id: Option<Uuid>,
        is_active: bool,
        public_key_base64: String,
        flake_id: Option<i32>,
        derivation: String,
    ) -> Result<Self> {
        let public_key = PublicKey::from_base64(&public_key_base64, &hostname)?;

        let now = Utc::now();
        Ok(System {
            id: Uuid::new_v4(),
            hostname,
            environment_id,
            is_active,
            public_key,
            flake_id,
            derivation,
            created_at: now,
            updated_at: now,
        })
    }

    /// Create a new System from a VerifyingKey
    pub fn from_verifying_key(
        hostname: String,
        environment_id: Option<Uuid>,
        is_active: bool,
        verifying_key: VerifyingKey,
        flake_id: Option<i32>,
        derivation: String,
    ) -> Self {
        let public_key = PublicKey::from_verifying_key(verifying_key);
        let now = Utc::now();

        System {
            id: Uuid::new_v4(),
            hostname,
            environment_id,
            is_active,
            public_key,
            flake_id,
            derivation,
            created_at: now,
            updated_at: now,
        }
    }

    /// Get the VerifyingKey (no longer needs Result since it's always valid)
    pub fn verifying_key(&self) -> &VerifyingKey {
        self.public_key.verifying_key()
    }

    /// Update the public key
    pub fn set_public_key(&mut self, public_key_base64: String) -> Result<()> {
        self.public_key = PublicKey::from_base64(&public_key_base64, &self.hostname)?;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Update the public key from a VerifyingKey
    pub fn set_verifying_key(&mut self, verifying_key: VerifyingKey) {
        self.public_key = PublicKey::from_verifying_key(verifying_key);
        self.updated_at = Utc::now();
    }

    /// Check if the system is active and can authenticate
    pub fn can_authenticate(&self) -> bool {
        self.is_active
        // No need to check if public_key is empty since PublicKey is always valid
    }

    /// Get the public key as base64 string (for compatibility)
    pub fn public_key_base64(&self) -> String {
        self.public_key.to_base64()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use ed25519_dalek::SigningKey;
//
//     #[test]
//     fn test_public_key_from_base64() {
//         let signing_key = SigningKey::generate(&mut rand::thread_rng());
//         let verifying_key = signing_key.verifying_key();
//         let base64_key = general_purpose::STANDARD.encode(verifying_key.to_bytes());
//
//         let public_key = PublicKey::from_base64(&base64_key, "test").unwrap();
//         assert_eq!(verifying_key.to_bytes(), public_key.to_bytes());
//     }
//
//     #[test]
//     fn test_public_key_invalid_base64() {
//         let result = PublicKey::from_base64("invalid-base64!", "test");
//         assert!(result.is_err());
//     }
//
//     #[test]
//     fn test_public_key_serialization() {
//         let signing_key = SigningKey::generate(&mut rand::thread_rng());
//         let verifying_key = signing_key.verifying_key();
//         let public_key = PublicKey::from_verifying_key(verifying_key);
//
//         // Test JSON serialization
//         let json = serde_json::to_string(&public_key).unwrap();
//         let deserialized: PublicKey = serde_json::from_str(&json).unwrap();
//
//         assert_eq!(public_key.to_bytes(), deserialized.to_bytes());
//     }
//
//     #[test]
//     fn test_new_system_with_valid_key() {
//         let signing_key = SigningKey::generate(&mut rand::thread_rng());
//         let verifying_key = signing_key.verifying_key();
//         let base64_key = general_purpose::STANDARD.encode(verifying_key.to_bytes());
//
//         let system = System::new(
//             "test-system".to_string(),
//             Some(Uuid::new_v4()),
//             true,
//             base64_key,
//             Some(1),
//             "/nix/store/test123".to_string(),
//         )
//         .unwrap();
//
//         assert_eq!(system.hostname, "test-system");
//         assert!(system.is_active);
//         assert!(system.can_authenticate());
//         assert_eq!(verifying_key.to_bytes(), system.verifying_key().to_bytes());
//     }
//
//     #[test]
//     fn test_new_system_with_invalid_key() {
//         let result = System::new(
//             "test-system".to_string(),
//             Some(Uuid::new_v4()),
//             true,
//             "invalid-base64!".to_string(),
//             Some(1),
//             "/nix/store/test123".to_string(),
//         );
//
//         assert!(result.is_err());
//     }
//
//     #[test]
//     fn test_from_verifying_key() {
//         let signing_key = SigningKey::generate(&mut rand::thread_rng());
//         let verifying_key = signing_key.verifying_key();
//
//         let system = System::from_verifying_key(
//             "test-system".to_string(),
//             Some(Uuid::new_v4()),
//             true,
//             verifying_key,
//             Some(1),
//             "/nix/store/test123".to_string(),
//         );
//
//         assert_eq!(verifying_key.to_bytes(), system.verifying_key().to_bytes());
//     }
//
//     #[test]
//     fn test_system_serialization() {
//         let signing_key = SigningKey::generate(&mut rand::thread_rng());
//         let verifying_key = signing_key.verifying_key();
//
//         let system = System::from_verifying_key(
//             "test-system".to_string(),
//             Some(Uuid::new_v4()),
//             true,
//             verifying_key,
//             Some(1),
//             "/nix/store/test123".to_string(),
//         );
//
//         // Test JSON serialization
//         let json = serde_json::to_string(&system).unwrap();
//         let deserialized: System = serde_json::from_str(&json).unwrap();
//
//         assert_eq!(system.hostname, deserialized.hostname);
//         assert_eq!(
//             system.public_key.to_bytes(),
//             deserialized.public_key.to_bytes()
//         );
//     }
//
//     #[test]
//     fn test_signature_verification() {
//         let signing_key = SigningKey::generate(&mut rand::thread_rng());
//         let verifying_key = signing_key.verifying_key();
//
//         let system = System::from_verifying_key(
//             "test-system".to_string(),
//             Some(Uuid::new_v4()),
//             true,
//             verifying_key,
//             Some(1),
//             "/nix/store/test123".to_string(),
//         );
//
//         // Test signature verification
//         let test_data = b"test system state data from Crystal Forge agent";
//         let signature = signing_key.sign(test_data);
//
//         assert!(system.verifying_key().verify(test_data, &signature).is_ok());
//
//         // Test with wrong data should fail
//         let wrong_data = b"wrong data";
//         assert!(
//             system
//                 .verifying_key()
//                 .verify(wrong_data, &signature)
//                 .is_err()
//         );
//     }
// }
