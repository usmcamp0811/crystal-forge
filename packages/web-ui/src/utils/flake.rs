//! Utilities for parsing and formatting flake references.

/// Parse a flake reference and extract human-readable parts.
///
/// Examples:
/// - `git+https://gitlab.com/org/repo?rev=abc123#nixosConfigurations.system-name`
///   -> FlakeRef { repo: "org/repo", commit: "abc123", system: Some("system-name") }
/// - `github:org/repo/abc123#nixosConfigurations.my-system`
///   -> FlakeRef { repo: "org/repo", commit: "abc123", system: Some("my-system") }
#[derive(Debug, Clone, PartialEq)]
pub struct FlakeRef {
    pub repo: String,
    pub commit: String,
    pub system: Option<String>,
}

impl FlakeRef {
    /// Parse a flake reference string into structured parts.
    pub fn parse(flake_ref: &str) -> Option<Self> {
        // Split on # to separate flake URL from output path
        let (url_part, output_part) = if let Some(idx) = flake_ref.find('#') {
            (&flake_ref[..idx], Some(&flake_ref[idx + 1..]))
        } else {
            (flake_ref, None)
        };

        // Extract system name from output path (e.g., "nixosConfigurations.my-system")
        let system = output_part.and_then(|out| {
            if out.starts_with("nixosConfigurations.") {
                Some(out.strip_prefix("nixosConfigurations.")?.to_string())
            } else {
                None
            }
        });

        // Parse the URL part to extract repo and commit
        let (repo, commit) = parse_url_part(url_part)?;

        Some(FlakeRef {
            repo,
            commit,
            system,
        })
    }

    /// Format as a short, readable string.
    ///
    /// Example: "org/repo @ abc123"
    pub fn short_format(&self) -> String {
        format!(
            "{} @ {}",
            self.repo,
            &self.commit[..7.min(self.commit.len())]
        )
    }

    /// Format as a short string with system name.
    ///
    /// Example: "org/repo @ abc123 · system-name"
    pub fn short_format_with_system(&self) -> String {
        if let Some(ref sys) = self.system {
            format!(
                "{} @ {} · {}",
                self.repo,
                &self.commit[..7.min(self.commit.len())],
                sys
            )
        } else {
            self.short_format()
        }
    }
}

/// Parse the URL part of a flake reference to extract repo and commit.
fn parse_url_part(url: &str) -> Option<(String, String)> {
    // Handle git+https://... or git+http://...
    if let Some(stripped) = url
        .strip_prefix("git+https://")
        .or_else(|| url.strip_prefix("git+http://"))
    {
        parse_git_url(stripped)
    }
    // Handle github:org/repo/commit or github:org/repo?rev=commit
    else if let Some(stripped) = url.strip_prefix("github:") {
        parse_github_url(stripped)
    }
    // Handle gitlab:org/repo/commit or gitlab:org/repo?rev=commit
    else if let Some(stripped) = url.strip_prefix("gitlab:") {
        parse_gitlab_url(stripped)
    }
    // Handle plain URLs
    else if url.starts_with("https://") || url.starts_with("http://") {
        parse_git_url(
            url.strip_prefix("https://")
                .or_else(|| url.strip_prefix("http://"))?,
        )
    } else {
        None
    }
}

/// Parse a git URL like "gitlab.com/org/repo?rev=abc123" or "gitlab.com/org/repo.git?rev=abc123"
fn parse_git_url(url: &str) -> Option<(String, String)> {
    // Split on ? to separate URL from query params
    let (path_part, query_part) = if let Some(idx) = url.find('?') {
        (&url[..idx], Some(&url[idx + 1..]))
    } else {
        (url, None)
    };

    // Extract commit from query params (e.g., "rev=abc123")
    let commit = query_part.and_then(|q| {
        q.split('&')
            .find_map(|param| param.strip_prefix("rev=").map(|s| s.to_string()))
    })?;

    // Extract repo from path (e.g., "gitlab.com/org/repo" or "gitlab.com/org/repo.git")
    let repo = extract_repo_from_path(path_part)?;

    Some((repo, commit))
}

/// Parse a github: URL like "org/repo/abc123" or "org/repo?rev=abc123"
fn parse_github_url(url: &str) -> Option<(String, String)> {
    // Check if it has ?rev= format
    if let Some(idx) = url.find("?rev=") {
        let repo = url[..idx].to_string();
        let commit = url[idx + 5..].split('&').next()?.to_string();
        return Some((repo, commit));
    }

    // Otherwise assume format is org/repo/commit
    let parts: Vec<&str> = url.split('/').collect();
    if parts.len() >= 3 {
        let repo = format!("{}/{}", parts[0], parts[1]);
        let commit = parts[2].to_string();
        Some((repo, commit))
    } else {
        None
    }
}

/// Parse a gitlab: URL (same format as github:)
fn parse_gitlab_url(url: &str) -> Option<(String, String)> {
    parse_github_url(url)
}

/// Extract "org/repo" from a full path like "gitlab.com/org/repo.git" or "github.com/org/repo"
fn extract_repo_from_path(path: &str) -> Option<String> {
    // Split by / and take the last 2 parts (org/repo)
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    if parts.len() < 2 {
        return None;
    }

    // Take the last two parts (org/repo)
    let org = parts[parts.len() - 2];
    let mut repo = parts[parts.len() - 1].to_string();

    // Remove .git suffix if present
    if repo.ends_with(".git") {
        repo = repo.strip_suffix(".git")?.to_string();
    }

    Some(format!("{}/{}", org, repo))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_git_https_url() {
        let input = "git+https://gitlab.com/crystal-forge/crystal-forge?rev=abc123456#nixosConfigurations.my-system";
        let parsed = FlakeRef::parse(input).unwrap();

        assert_eq!(parsed.repo, "crystal-forge/crystal-forge");
        assert_eq!(parsed.commit, "abc123456");
        assert_eq!(parsed.system, Some("my-system".to_string()));
        assert_eq!(
            parsed.short_format(),
            "crystal-forge/crystal-forge @ abc1234"
        );
        assert_eq!(
            parsed.short_format_with_system(),
            "crystal-forge/crystal-forge @ abc1234 · my-system"
        );
    }

    #[test]
    fn test_parse_github_url() {
        let input = "github:nixos/nixpkgs/abc123#nixosConfigurations.laptop";
        let parsed = FlakeRef::parse(input).unwrap();

        assert_eq!(parsed.repo, "nixos/nixpkgs");
        assert_eq!(parsed.commit, "abc123");
        assert_eq!(parsed.system, Some("laptop".to_string()));
    }

    #[test]
    fn test_parse_with_git_suffix() {
        let input =
            "git+https://gitlab.com/usmcamp0811/dotfiles.git?rev=deadbeef#nixosConfigurations.gray";
        let parsed = FlakeRef::parse(input).unwrap();

        assert_eq!(parsed.repo, "usmcamp0811/dotfiles");
        assert_eq!(parsed.commit, "deadbeef");
        assert_eq!(parsed.system, Some("gray".to_string()));
    }

    #[test]
    fn test_parse_without_system() {
        let input = "git+https://gitlab.com/org/repo?rev=abc123";
        let parsed = FlakeRef::parse(input).unwrap();

        assert_eq!(parsed.repo, "org/repo");
        assert_eq!(parsed.commit, "abc123");
        assert_eq!(parsed.system, None);
        assert_eq!(parsed.short_format(), "org/repo @ abc123");
    }
}
