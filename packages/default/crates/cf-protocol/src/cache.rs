//! Cache-related wire types shared between server and builder.

use serde::{Deserialize, Serialize};

/// Type of cache destination.
///
/// Used in `BuilderCachePushConfig` delivered from server to builder and in the
/// server-side `CacheConfig` TOML section.
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub enum CacheType {
    S3,
    Attic,
    Http,
    #[default]
    Nix,
}
