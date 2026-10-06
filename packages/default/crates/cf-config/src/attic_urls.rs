//! Derives Attic login, read, and cache-specific API URLs without network access.
//!
//! This module normalizes legacy representations in memory. Callers retain the
//! original configuration and enforce transport, credential-query, DNS, and
//! redirect policy before using the derived URLs.

use std::fmt;
use url::Url;

/// Holds endpoints for one actual Attic cache beneath a server directory.
///
/// Values returned by [`resolve_attic_urls`] share an authority, HTTP scheme,
/// server prefix, and query. The server URL ends in `/`; the other paths do not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtticUrls {
    /// Server directory used for login, including any reverse-proxy prefix.
    pub server_url: Url,
    /// Canonical public Nix read root for the selected cache.
    pub cache_url: Url,
    /// Nix metadata endpoint beneath the canonical cache read root.
    pub metadata_url: Url,
    /// Read-only `_api/v1/cache-config/<cache>` endpoint under the server prefix.
    pub cache_config_url: Url,
    /// Actual native cache name, without a configured remote-reference prefix.
    pub cache_name: String,
}

/// Identifies URL resolution failures without retaining sensitive input.
///
/// Both `Display` and `Debug` contain only static information. URL parser errors
/// and input strings are not retained as error sources.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtticUrlError {
    /// The name is not native Attic syntax or a valid `remote:cache` reference.
    InvalidCacheName,
    /// The input is not an absolute HTTP, HTTPS, or legacy Attic URL with a host.
    InvalidServerUrl,
    /// The URL contains userinfo, including syntactically empty userinfo.
    UserInfoNotAllowed,
    /// The URL contains a fragment, including an empty fragment.
    FragmentNotAllowed,
}

impl fmt::Display for AtticUrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidCacheName => "Invalid Attic cache name or remote reference",
            Self::InvalidServerUrl => "Invalid Attic server URL",
            Self::UserInfoNotAllowed => "Attic URL userinfo is not supported",
            Self::FragmentNotAllowed => "Attic URL fragments are not supported",
        })
    }
}

impl std::error::Error for AtticUrlError {}

/// Resolves canonical Attic endpoints from a server URL and configured name.
///
/// Accepts HTTP(S) server bases, matching cache roots with an optional trailing
/// slash, and matching `<cache>/nix-cache-info` compatibility URLs. Only one
/// matching final cache segment is removed. An unrelated path remains a server
/// prefix. Percent-encoded final segments are compared as decoded ASCII.
/// Paths are extended by individual URL segments, preserving proxy prefixes.
///
/// Native names contain 1–50 ASCII characters: an initial alphanumeric character
/// followed by alphanumeric characters, `_`, `+`, or `-`. A configured
/// `remote:cache` reference must have exactly one colon and a nonempty remote;
/// only its validated cache component is used in endpoints and `cache_name`.
/// The caller's original configuration is not modified.
///
/// # Compatibility
///
/// Legacy `attic://` URLs use HTTPS and discard their **entire path**, preserving
/// historical login semantics. The configured cache name remains authoritative.
/// HTTP remains HTTP for CLI and read consumers; an HTTPS-only API probe policy
/// belongs to the caller. Queries, including empty queries, are preserved on
/// all four URLs, not dropped or concatenated. This helper does not classify
/// credential queries; server callers must apply that policy before DNS access.
///
/// # Errors
///
/// Returns [`AtticUrlError`] for invalid names or remote references, unsupported
/// or malformed URLs, missing hosts, userinfo, or fragments. Error messages
/// never include input URLs, names, credentials, or parser error text.
///
/// # Examples
///
/// ```
/// use cf_config::resolve_attic_urls;
///
/// let urls = resolve_attic_urls("https://cache.example/proxy/", "local:team")?;
/// assert_eq!(urls.server_url.as_str(), "https://cache.example/proxy/");
/// assert_eq!(urls.cache_url.as_str(), "https://cache.example/proxy/team");
/// assert_eq!(urls.cache_config_url.as_str(),
///            "https://cache.example/proxy/_api/v1/cache-config/team");
/// assert_eq!(urls.cache_name, "team");
/// # Ok::<(), cf_config::AtticUrlError>(())
/// ```
pub fn resolve_attic_urls(push_to: &str, cache_name: &str) -> Result<AtticUrls, AtticUrlError> {
    let cache_name = actual_cache_name(cache_name)?;
    let (scheme, remainder) = push_to
        .split_once("://")
        .ok_or(AtticUrlError::InvalidServerUrl)?;
    let alias = scheme.eq_ignore_ascii_case("attic");
    if !alias && !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return Err(AtticUrlError::InvalidServerUrl);
    }
    // SECURITY: Url normalizes empty userinfo away. Inspect the authority first
    // so even `https://@host` cannot bypass the userinfo prohibition.
    let authority = remainder.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.contains('@') {
        return Err(AtticUrlError::UserInfoNotAllowed);
    }
    let input = if alias {
        format!("https://{remainder}")
    } else {
        push_to.to_owned()
    };
    let mut server_url = Url::parse(&input).map_err(|_| AtticUrlError::InvalidServerUrl)?;
    if server_url.host_str().is_none() || authority.is_empty() {
        return Err(AtticUrlError::InvalidServerUrl);
    }
    if !server_url.username().is_empty() || server_url.password().is_some() {
        return Err(AtticUrlError::UserInfoNotAllowed);
    }
    if server_url.fragment().is_some() {
        return Err(AtticUrlError::FragmentNotAllowed);
    }
    if alias {
        // COMPATIBILITY: Attic aliases historically select the authority only,
        // even when their path looks like a reverse-proxy prefix.
        server_url.set_path("/");
    } else {
        let segments: Vec<_> = server_url
            .path_segments()
            .ok_or(AtticUrlError::InvalidServerUrl)?
            .collect();
        let end = segments.len() - usize::from(segments.last() == Some(&""));
        let strip = if end >= 2
            && segment_matches(segments[end - 1], "nix-cache-info")
            && segment_matches(segments[end - 2], cache_name)
        {
            2
        } else if end >= 1 && segment_matches(segments[end - 1], cache_name) {
            1
        } else {
            0
        };
        let mut path = server_url
            .path_segments_mut()
            .map_err(|_| AtticUrlError::InvalidServerUrl)?;
        path.pop_if_empty();
        for _ in 0..strip {
            path.pop();
        }
        path.push("");
    }
    let cache_url = append_segments(&server_url, &[cache_name])?;
    let metadata_url = append_segments(&cache_url, &["nix-cache-info"])?;
    let cache_config_url =
        append_segments(&server_url, &["_api", "v1", "cache-config", cache_name])?;
    Ok(AtticUrls {
        server_url,
        cache_url,
        metadata_url,
        cache_config_url,
        cache_name: cache_name.to_owned(),
    })
}

fn actual_cache_name(configured: &str) -> Result<&str, AtticUrlError> {
    let name = match configured.split_once(':') {
        Some((remote, cache)) if !remote.is_empty() && !cache.contains(':') => cache,
        Some(_) => return Err(AtticUrlError::InvalidCacheName),
        None => configured,
    };
    let bytes = name.as_bytes();
    if !(1..=50).contains(&bytes.len())
        || !bytes[0].is_ascii_alphanumeric()
        || !bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"_+-".contains(b))
    {
        return Err(AtticUrlError::InvalidCacheName);
    }
    Ok(name)
}

fn append_segments(base: &Url, segments: &[&str]) -> Result<Url, AtticUrlError> {
    let mut url = base.clone();
    url.path_segments_mut()
        .map_err(|_| AtticUrlError::InvalidServerUrl)?
        .pop_if_empty()
        .extend(segments.iter().copied());
    Ok(url)
}

// Only validated ASCII names and the ASCII metadata suffix are compared. Decode
// escapes for comparison only; retain the original encoded proxy path in Url.
fn segment_matches(encoded: &str, expected: &str) -> bool {
    let mut input = encoded.bytes();
    for wanted in expected.bytes() {
        let actual = match input.next() {
            Some(b'%') => {
                let Some(high) = input.next().and_then(hex_digit) else {
                    return false;
                };
                let Some(low) = input.next().and_then(hex_digit) else {
                    return false;
                };
                high * 16 + low
            }
            Some(byte) => byte,
            None => return false,
        };
        if actual != wanted {
            return false;
        }
    }
    input.next().is_none()
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_input_matrix() {
        for (input, base) in [
            ("https://cache.example", "https://cache.example/"),
            ("https://cache.example/", "https://cache.example/"),
            ("https://cache.example/team", "https://cache.example/"),
            ("https://cache.example/team/", "https://cache.example/"),
            (
                "https://cache.example/team/nix-cache-info",
                "https://cache.example/",
            ),
            ("attic://cache.example", "https://cache.example/"),
            ("attic://cache.example:8443/", "https://cache.example:8443/"),
            ("attic://cache.example/proxy/team", "https://cache.example/"),
            ("http://cache.example/proxy", "http://cache.example/proxy/"),
            ("http://cache.example/proxy/", "http://cache.example/proxy/"),
            (
                "http://cache.example/proxy/team",
                "http://cache.example/proxy/",
            ),
            (
                "http://cache.example/proxy/team/",
                "http://cache.example/proxy/",
            ),
            (
                "http://cache.example/proxy/team/nix-cache-info",
                "http://cache.example/proxy/",
            ),
            (
                "https://cache.example/team/team",
                "https://cache.example/team/",
            ),
        ] {
            let urls = resolve_attic_urls(input, "team").unwrap();
            assert_eq!(urls.server_url.as_str(), base, "{input}");
            assert_eq!(urls.cache_url.as_str(), format!("{base}team"));
            assert_eq!(
                urls.metadata_url.as_str(),
                format!("{base}team/nix-cache-info")
            );
            assert_eq!(
                urls.cache_config_url.as_str(),
                format!("{base}_api/v1/cache-config/team")
            );
            assert_eq!(urls.cache_name, "team");
        }
    }

    #[test]
    fn queries_are_preserved_on_every_endpoint() {
        for query in ["?region=west&label=a%2Fb", "?"] {
            for path in ["/proxy/", "/proxy/team/", "/proxy/team/nix-cache-info"] {
                let urls =
                    resolve_attic_urls(&format!("http://cache.example{path}{query}"), "team")
                        .unwrap();
                for (url, path) in [
                    (urls.server_url, "/proxy/"),
                    (urls.cache_url, "/proxy/team"),
                    (urls.metadata_url, "/proxy/team/nix-cache-info"),
                    (urls.cache_config_url, "/proxy/_api/v1/cache-config/team"),
                ] {
                    assert_eq!(url.as_str(), format!("http://cache.example{path}{query}"));
                }
            }
            let urls = resolve_attic_urls(
                &format!("attic://cache.example:8443/proxy/team{query}"),
                "team",
            )
            .unwrap();
            assert_eq!(
                urls.server_url.as_str(),
                format!("https://cache.example:8443/{query}")
            );
            assert_eq!(urls.cache_config_url.query(), urls.server_url.query());
            assert_eq!(urls.cache_url.query(), urls.server_url.query());
            assert_eq!(urls.metadata_url.query(), urls.server_url.query());
        }
    }

    #[test]
    fn encoded_matching_and_prefixes_do_not_duplicate_or_split_names() {
        for path in [
            "/p%2Frefix/t%65am",
            "/p%2Frefix/%74eam/",
            "/p%2Frefix/team/nix-cache-%69nfo",
        ] {
            let urls =
                resolve_attic_urls(&format!("https://cache.example{path}"), "remote:team").unwrap();
            assert_eq!(urls.server_url.path(), "/p%2Frefix/");
            assert_eq!(urls.cache_url.path(), "/p%2Frefix/team");
            assert_eq!(
                urls.cache_config_url.path(),
                "/p%2Frefix/_api/v1/cache-config/team"
            );
            assert_eq!(urls.cache_name, "team");
        }
        let urls =
            resolve_attic_urls("https://cache.example/prefix/A%2bb_1-", "remote:A+b_1-").unwrap();
        assert_eq!(urls.server_url.path(), "/prefix/");
        assert_eq!(urls.cache_url.path_segments().unwrap().count(), 2);
        assert_eq!(urls.cache_config_url.path_segments().unwrap().count(), 5);
        assert_eq!(urls.cache_url.path(), "/prefix/A+b_1-");
    }

    #[test]
    fn unrelated_or_partial_paths_remain_server_prefixes() {
        for path in [
            "/nix-cache-info",
            "/prefix/nix-cache-info",
            "/teammate",
            "/other/nix-cache-info",
            "/team%2Fextra",
            "/%74eam%",
            "/%74eam%GG",
        ] {
            let urls = resolve_attic_urls(&format!("https://cache.example{path}"), "team").unwrap();
            assert_eq!(urls.server_url.path(), format!("{path}/"));
            assert_eq!(urls.cache_url.path(), format!("{path}/team"));
        }
    }

    #[test]
    fn native_names_and_remote_references_are_validated() {
        for name in ["a", "9", "A+b_c-0", &"a".repeat(50), "remote:team"] {
            assert!(resolve_attic_urls("https://cache.example/", name).is_ok());
        }
        for name in [
            "",
            "_team",
            "-team",
            "+team",
            "team/name",
            "team%2Fname",
            "téam",
            "team name",
            &"a".repeat(51),
            ":team",
            "remote:",
            "remote:other:team",
        ] {
            assert_eq!(
                resolve_attic_urls("https://cache.example/", name),
                Err(AtticUrlError::InvalidCacheName)
            );
        }
        let raw = "remote:team";
        let urls = resolve_attic_urls("https://cache.example/team/", raw).unwrap();
        assert_eq!(raw, "remote:team");
        assert_eq!(urls.cache_url.path(), "/team");
        assert_eq!(urls.cache_config_url.path(), "/_api/v1/cache-config/team");
    }

    #[test]
    fn rejects_userinfo_fragments_and_invalid_urls_with_static_errors() {
        for scheme in ["http", "https", "attic"] {
            for userinfo in [
                "@",
                "user@",
                ":@",
                ":private-marker@",
                "user:private-marker@",
            ] {
                let error =
                    resolve_attic_urls(&format!("{scheme}://{userinfo}cache.example/"), "team")
                        .unwrap_err();
                assert_eq!(error, AtticUrlError::UserInfoNotAllowed);
                assert_eq!(error.to_string(), "Attic URL userinfo is not supported");
                assert!(!format!("{error:?}").contains("private-marker"));
            }
            for fragment in ["#", "#private-marker"] {
                assert_eq!(
                    resolve_attic_urls(&format!("{scheme}://cache.example/{fragment}"), "team"),
                    Err(AtticUrlError::FragmentNotAllowed)
                );
            }
        }
        for input in [
            "not a URL private-marker",
            "ftp://cache.example",
            "https:///",
            "https://",
            "https://cache.example:invalid",
        ] {
            let error = resolve_attic_urls(input, "team").unwrap_err();
            assert_eq!(error, AtticUrlError::InvalidServerUrl);
            assert_eq!(error.to_string(), "Invalid Attic server URL");
        }
    }

    #[test]
    fn public_read_and_api_consumers_share_the_same_canonical_root() {
        let base = resolve_attic_urls("https://cache.example/prefix/", "team").unwrap();
        for input in [base.cache_url.as_str(), base.metadata_url.as_str()] {
            assert_eq!(resolve_attic_urls(input, "team").unwrap(), base);
        }
    }
}
