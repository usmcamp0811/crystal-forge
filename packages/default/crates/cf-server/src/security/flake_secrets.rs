use anyhow::Result;

pub fn encrypt_secret(value: &str) -> Result<String> {
    super::cache_secrets::encrypt_secret(value)
}

pub fn decrypt_secret(value: &str) -> Result<String> {
    super::cache_secrets::decrypt_secret(value)
}

pub fn encrypt_optional(value: Option<&str>) -> Result<Option<String>> {
    super::cache_secrets::encrypt_optional(value)
}

pub fn decrypt_optional(value: Option<&str>) -> Result<Option<String>> {
    super::cache_secrets::decrypt_optional(value)
}

pub fn is_encrypted(value: &str) -> bool {
    super::cache_secrets::is_encrypted(value)
}
