pub(crate) fn normalize_tenant_discriminator(value: Option<&str>) -> String {
    value.unwrap_or_default().to_string()
}

#[cfg(test)]
mod tests {
    use super::normalize_tenant_discriminator;

    #[test]
    fn auth_repository_tenant_normalization_is_stable() {
        assert_eq!(normalize_tenant_discriminator(None), "");
        assert_eq!(normalize_tenant_discriminator(Some("tenant-a")), "tenant-a");
    }
}
