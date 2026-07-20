use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct ScanPackage {
    pub id: Uuid,
    pub scan_id: Uuid,
    pub derivation_path: String,
    pub is_runtime_dependency: bool,
    pub dependency_depth: i32, // 0 = direct, 1+ = transitive
    pub created_at: DateTime<Utc>,
}

impl ScanPackage {
    /// Create a new scan package relationship
    pub fn new(
        scan_id: Uuid,
        derivation_path: String,
        is_runtime_dependency: bool,
        dependency_depth: i32,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            scan_id,
            derivation_path,
            is_runtime_dependency,
            dependency_depth,
            created_at: Utc::now(),
        }
    }

    /// Check if this is a direct dependency
    pub fn is_direct_dependency(&self) -> bool {
        self.dependency_depth == 0
    }

    /// Check if this is a transitive dependency
    pub fn is_transitive_dependency(&self) -> bool {
        self.dependency_depth > 0
    }

    /// Get dependency type as string
    pub fn dependency_type(&self) -> &'static str {
        if self.is_direct_dependency() {
            "direct"
        } else {
            "transitive"
        }
    }

    /// Get runtime classification
    pub fn runtime_classification(&self) -> &'static str {
        if self.is_runtime_dependency {
            "runtime"
        } else {
            "build-time"
        }
    }
}

impl std::fmt::Display for ScanPackage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({} dependency, depth {})",
            self.derivation_path,
            self.dependency_type(),
            self.dependency_depth
        )
    }
}
