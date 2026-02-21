//! Password hashing and verification for local authentication.
//!
//! Uses Argon2id (winner of the Password Hashing Competition) for secure password storage.

use anyhow::{Context, Result};
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};

/// Hash a plaintext password using Argon2id.
///
/// Returns the PHC (Password Hashing Competition) string format hash that includes:
/// - Algorithm identifier (argon2id)
/// - Parameters (memory, iterations, parallelism)
/// - Salt
/// - Hash
///
/// # Example
///
/// ```no_run
/// use crystal_forge::auth::password::hash_password;
///
/// let hash = hash_password("my-secure-password").unwrap();
/// assert!(hash.starts_with("$argon2id$"));
/// ```
pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();

    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("Failed to hash password: {}", e))?
        .to_string();

    Ok(password_hash)
}

/// Verify a plaintext password against a stored hash.
///
/// Returns `Ok(())` if the password matches, `Err` otherwise.
///
/// # Example
///
/// ```no_run
/// use crystal_forge::auth::password::{hash_password, verify_password};
///
/// let hash = hash_password("my-secure-password").unwrap();
/// assert!(verify_password("my-secure-password", &hash).is_ok());
/// assert!(verify_password("wrong-password", &hash).is_err());
/// ```
pub fn verify_password(password: &str, password_hash: &str) -> Result<()> {
    let parsed_hash = PasswordHash::new(password_hash)
        .map_err(|e| anyhow::anyhow!("Invalid password hash format: {}", e))?;

    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .map_err(|e| anyhow::anyhow!("Password verification failed: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_password_produces_argon2id_hash() {
        let hash = hash_password("test-password").unwrap();
        assert!(hash.starts_with("$argon2id$"));
    }

    #[test]
    fn verify_password_succeeds_with_correct_password() {
        let password = "correct-horse-battery-staple";
        let hash = hash_password(password).unwrap();
        assert!(verify_password(password, &hash).is_ok());
    }

    #[test]
    fn verify_password_fails_with_wrong_password() {
        let hash = hash_password("correct-password").unwrap();
        assert!(verify_password("wrong-password", &hash).is_err());
    }

    #[test]
    fn hash_password_produces_different_hashes_for_same_password() {
        let password = "same-password";
        let hash1 = hash_password(password).unwrap();
        let hash2 = hash_password(password).unwrap();

        // Hashes should be different due to random salt
        assert_ne!(hash1, hash2);

        // But both should verify
        assert!(verify_password(password, &hash1).is_ok());
        assert!(verify_password(password, &hash2).is_ok());
    }

    #[test]
    fn verify_password_fails_with_invalid_hash_format() {
        let result = verify_password("password", "not-a-valid-hash");
        assert!(result.is_err());
    }
}
