use crate::api::models::NixosOptionMetadata;
use crate::models::deployment_policies::{
    CompositePolicyConfig, CompositeRuleKind, NixosOptionValueType,
};
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::sync::Arc;

pub const METADATA_PATH_ENV: &str = "CRYSTAL_FORGE_NIXOS_OPTIONS_METADATA";
pub const DEFAULT_SEARCH_LIMIT: usize = 20;
pub const MAX_SEARCH_LIMIT: usize = 100;

/// Packaged entries reflect Crystal Forge's pinned nixpkgs and are authoring
/// guidance only. A monitored target can use a different nixpkgs revision or
/// additional modules; evaluation of that target remains authoritative.
#[derive(Clone)]
pub struct NixosOptionsMetadataProvider {
    state: Arc<ProviderState>,
}

enum ProviderState {
    Available(Vec<IndexedEntry>),
    Unavailable(String),
    Corrupt(String),
}

struct IndexedEntry {
    metadata: NixosOptionMetadata,
    normalized_path: String,
    normalized_description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataProviderError {
    Unavailable(String),
    Corrupt(String),
}

impl Display for MetadataProviderError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(message) | Self::Corrupt(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for MetadataProviderError {}

impl NixosOptionsMetadataProvider {
    pub fn from_runtime() -> Self {
        let path = std::env::var(METADATA_PATH_ENV)
            .ok()
            .filter(|path| !path.trim().is_empty())
            .or_else(|| option_env!("CRYSTAL_FORGE_NIXOS_OPTIONS_METADATA").map(ToOwned::to_owned));

        match path {
            Some(path) => Self::from_path(path),
            None => Self {
                state: Arc::new(ProviderState::Unavailable(format!(
                    "neither runtime {METADATA_PATH_ENV} nor a packaged metadata path is available"
                ))),
            },
        }
    }

    pub fn from_path(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        match std::fs::read(path) {
            Ok(contents) => Self::from_json_bytes(&contents).unwrap_or_else(|error| Self {
                state: Arc::new(ProviderState::Corrupt(format!(
                    "failed to parse {}: {error}",
                    path.display()
                ))),
            }),
            Err(error) => Self {
                state: Arc::new(ProviderState::Unavailable(format!(
                    "failed to read {}: {error}",
                    path.display()
                ))),
            },
        }
    }

    pub fn from_json_bytes(contents: &[u8]) -> Result<Self, serde_json::Error> {
        let mut entries: Vec<NixosOptionMetadata> = serde_json::from_slice(contents)?;
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        let entries = entries
            .into_iter()
            .map(|metadata| IndexedEntry {
                normalized_path: metadata.path.to_lowercase(),
                normalized_description: metadata
                    .description
                    .as_ref()
                    .map(|value| value.to_lowercase()),
                metadata,
            })
            .collect();
        Ok(Self {
            state: Arc::new(ProviderState::Available(entries)),
        })
    }

    pub fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<NixosOptionMetadata>, MetadataProviderError> {
        let entries = match self.state.as_ref() {
            ProviderState::Available(entries) => entries,
            ProviderState::Unavailable(message) => {
                return Err(MetadataProviderError::Unavailable(message.clone()));
            }
            ProviderState::Corrupt(message) => {
                return Err(MetadataProviderError::Corrupt(message.clone()));
            }
        };
        let query = query.trim().to_lowercase();
        let limit = limit.clamp(1, MAX_SEARCH_LIMIT);
        let mut matches = entries
            .iter()
            .filter_map(|entry| match_rank(entry, &query).map(|rank| (rank, entry)))
            .collect::<Vec<_>>();
        matches.sort_by(|(left_rank, left), (right_rank, right)| {
            left_rank
                .cmp(right_rank)
                .then_with(|| left.metadata.path.cmp(&right.metadata.path))
        });
        Ok(matches
            .into_iter()
            .take(limit)
            .map(|(_, entry)| entry.metadata.clone())
            .collect())
    }

    pub fn get_exact(
        &self,
        path: &str,
    ) -> Result<Option<NixosOptionMetadata>, MetadataProviderError> {
        let entries = match self.state.as_ref() {
            ProviderState::Available(entries) => entries,
            ProviderState::Unavailable(message) => {
                return Err(MetadataProviderError::Unavailable(message.clone()));
            }
            ProviderState::Corrupt(message) => {
                return Err(MetadataProviderError::Corrupt(message.clone()));
            }
        };
        Ok(entries
            .binary_search_by(|entry| entry.metadata.path.as_str().cmp(path))
            .ok()
            .map(|index| entries[index].metadata.clone()))
    }

    /// Validate authoring/serialization semantics against CF's baseline
    /// catalog. This does not prove target validity. When the baseline is
    /// missing an entry, unavailable, or corrupt, the explicit `unknown`
    /// semantic-string path preserves authoring for foreign module graphs.
    pub fn validate_composite_config(&self, config: &CompositePolicyConfig) -> Result<(), String> {
        for (index, rule) in config.rules.iter().enumerate() {
            let CompositeRuleKind::NixosOption(option) = &rule.rule else {
                continue;
            };
            let metadata = match self.get_exact(&option.path) {
                Ok(metadata) => metadata,
                Err(MetadataProviderError::Unavailable(_) | MetadataProviderError::Corrupt(_))
                    if option.value_type == NixosOptionValueType::Unknown =>
                {
                    continue;
                }
                Err(MetadataProviderError::Unavailable(message)) => {
                    return Err(format!(
                        "composite config.rules[{index}] cannot verify option '{}' because metadata is unavailable ({message}); use value_type unknown",
                        option.path
                    ));
                }
                Err(MetadataProviderError::Corrupt(message)) => {
                    return Err(format!(
                        "composite config.rules[{index}] cannot verify option '{}' because metadata is corrupt ({message}); use value_type unknown",
                        option.path
                    ));
                }
            };
            let Some(metadata) = metadata else {
                if option.value_type != NixosOptionValueType::Unknown {
                    return Err(format!(
                        "composite config.rules[{index}] option '{}' is not in metadata; use value_type unknown",
                        option.path
                    ));
                }
                continue;
            };
            let expected = match metadata.value_type {
                crate::api::models::NixosOptionValueType::Boolean => NixosOptionValueType::Boolean,
                crate::api::models::NixosOptionValueType::Enum => NixosOptionValueType::Enum,
                crate::api::models::NixosOptionValueType::Integer => NixosOptionValueType::Integer,
                crate::api::models::NixosOptionValueType::String => NixosOptionValueType::String,
                crate::api::models::NixosOptionValueType::Lines => NixosOptionValueType::Lines,
                crate::api::models::NixosOptionValueType::Unknown => NixosOptionValueType::Unknown,
            };
            if option.value_type != expected {
                return Err(format!(
                    "composite config.rules[{index}] value_type does not match metadata for '{}'",
                    option.path
                ));
            }
            if expected == NixosOptionValueType::Enum
                && !metadata.enum_values.is_empty()
                && !metadata
                    .enum_values
                    .iter()
                    .any(|value| value == &option.value)
            {
                return Err(format!(
                    "composite config.rules[{index}] enum value is not allowed for '{}'",
                    option.path
                ));
            }
        }
        Ok(())
    }
}

fn match_rank(entry: &IndexedEntry, query: &str) -> Option<u8> {
    if query.is_empty() {
        return Some(0);
    }
    if entry.normalized_path == query {
        Some(0)
    } else if entry.normalized_path.starts_with(query) {
        Some(1)
    } else if entry.normalized_path.contains(query) {
        Some(2)
    } else if entry
        .normalized_description
        .as_deref()
        .is_some_and(|description| description.contains(query))
    {
        Some(3)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &[u8] = br#"[
      {"path":"z.last","value_type":"string","description":"Searchable firewall prose"},
      {"path":"networking.firewall.enable","value_type":"boolean"},
      {"path":"networking.firewall.backend","value_type":"enum","enum_values":["iptables","nftables"]}
    ]"#;

    #[test]
    fn indexes_once_and_searches_path_then_description_deterministically() {
        let provider = NixosOptionsMetadataProvider::from_json_bytes(FIXTURE).unwrap();
        let results = provider.search("firewall", 10).unwrap();
        assert_eq!(
            results
                .iter()
                .map(|entry| entry.path.as_str())
                .collect::<Vec<_>>(),
            [
                "networking.firewall.backend",
                "networking.firewall.enable",
                "z.last"
            ]
        );
    }

    #[test]
    fn search_limit_is_bounded_and_zero_is_not_unbounded() {
        let provider = NixosOptionsMetadataProvider::from_json_bytes(FIXTURE).unwrap();
        assert_eq!(provider.search("", 0).unwrap().len(), 1);
        assert_eq!(provider.search("", usize::MAX).unwrap().len(), 3);
    }

    #[test]
    fn distinguishes_unavailable_and_corrupt_sources() {
        let unavailable = NixosOptionsMetadataProvider::from_path("/definitely/not/present.json");
        assert!(matches!(
            unavailable.search("", 1),
            Err(MetadataProviderError::Unavailable(_))
        ));

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("metadata.json");
        std::fs::write(&path, b"not json").unwrap();
        let corrupt = NixosOptionsMetadataProvider::from_path(path);
        assert!(matches!(
            corrupt.search("", 1),
            Err(MetadataProviderError::Corrupt(_))
        ));
    }

    #[test]
    fn validates_known_types_and_enums() {
        let provider = NixosOptionsMetadataProvider::from_json_bytes(FIXTURE).unwrap();
        let config: CompositePolicyConfig = serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "mode": "all",
            "rules": [{
                "id": "10000000-0000-0000-0000-000000000001",
                "kind": "nixos_option",
                "config": {
                    "path": "networking.firewall.backend",
                    "operator": "==",
                    "value_type": "enum",
                    "value": "nftables"
                }
            }]
        }))
        .unwrap();
        provider.validate_composite_config(&config).unwrap();

        let mut wrong = config.clone();
        if let CompositeRuleKind::NixosOption(option) = &mut wrong.rules[0].rule {
            option.value = serde_json::json!("unknown-backend");
        } else {
            unreachable!();
        }
        assert!(provider.validate_composite_config(&wrong).is_err());
    }

    #[test]
    fn baseline_absence_preserves_unknown_custom_authoring_path() {
        let provider = NixosOptionsMetadataProvider::from_json_bytes(FIXTURE).unwrap();
        let config: CompositePolicyConfig = serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "mode": "all",
            "rules": [{
                "id": "10000000-0000-0000-0000-000000000001",
                "kind": "nixos_option",
                "config": {
                    "path": "acme.security.fips.enable",
                    "operator": "==",
                    "value_type": "unknown",
                    "value": "true"
                }
            }]
        }))
        .unwrap();

        assert!(
            provider
                .get_exact("acme.security.fips.enable")
                .unwrap()
                .is_none()
        );
        assert!(provider.validate_composite_config(&config).is_ok());
    }

    #[test]
    fn unavailable_metadata_only_allows_unknown_string_fallback() {
        let provider = NixosOptionsMetadataProvider::from_path("/definitely/not/present.json");
        let mut config: CompositePolicyConfig = serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "mode": "all",
            "rules": [{
                "id": "10000000-0000-0000-0000-000000000001",
                "kind": "nixos_option",
                "config": {
                    "path": "networking.firewall.enable",
                    "operator": "==",
                    "value_type": "boolean",
                    "value": true
                }
            }]
        }))
        .unwrap();

        assert!(provider.validate_composite_config(&config).is_err());
        if let CompositeRuleKind::NixosOption(option) = &mut config.rules[0].rule {
            option.value_type = NixosOptionValueType::Unknown;
            option.value = serde_json::json!("true");
        } else {
            unreachable!();
        }
        assert!(provider.validate_composite_config(&config).is_ok());
    }
}
