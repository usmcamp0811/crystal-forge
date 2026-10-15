//! Signs bounded, read-only S3 bucket probes with explicit in-memory credentials.
//!
//! No SDK, profile, environment credentials, or metadata service is consulted.

use super::*;
use chrono::{DateTime, Utc};
use reqwest::header::{AUTHORIZATION, HOST, HeaderMap, HeaderValue};
use ring::hmac;
use sha2::{Digest, Sha256};

fn digest(value: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(value.as_ref()))
}

fn mac(key: &[u8], value: &str) -> Vec<u8> {
    hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, key), value.as_bytes())
        .as_ref()
        .to_vec()
}

fn header(value: &str, sensitive: bool) -> Result<HeaderValue, String> {
    let mut header = HeaderValue::from_str(value)
        .map_err(|_| "Invalid S3 credential or signing header".to_string())?;
    header.set_sensitive(sensitive);
    Ok(header)
}

// SECURITY: Sign exactly the URL and headers sent by reqwest. Restrict the
// endpoint to an unsigned base URL and append the bucket as one encoded segment.
// S3 paths are not normalized or double-encoded during canonicalization.
/// Constructs an explicit-key SigV4 GET without contacting the bucket.
///
/// The URL preserves the endpoint prefix and uses path-style bucket addressing.
/// Returned headers contain credentials and must not be logged or serialized.
///
/// # Errors
/// Returns a credential-free error for malformed URLs or signing inputs.
pub(super) fn signed_bucket_request(
    create: &CreateCacheDestination,
    now: DateTime<Utc>,
) -> Result<(Url, HeaderMap), String> {
    let mut url = Url::parse(
        create
            .s3_endpoint_url
            .as_deref()
            .ok_or("Missing S3 endpoint")?,
    )
    .map_err(|_| "Invalid S3 endpoint URL")?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("S3 endpoint must not contain userinfo, queries, or fragments".into());
    }
    let bucket_url = Url::parse(create.push_to.as_deref().ok_or("Missing S3 bucket URL")?)
        .map_err(|_| "Invalid S3 bucket URL")?;
    if bucket_url.scheme() != "s3"
        || !bucket_url.username().is_empty()
        || bucket_url.password().is_some()
        || bucket_url.fragment().is_some()
        || bucket_url
            .query_pairs()
            .any(|(name, _)| cache_url_query_parameter_is_sensitive(&name))
    {
        return Err("S3 probe requires a credential-free s3://bucket URL".into());
    }
    let bucket = bucket_url.host_str().ok_or("Missing S3 bucket")?;
    if bucket.is_empty()
        || !bucket
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".-".contains(&c))
    {
        return Err("Invalid S3 bucket name".into());
    }
    url.path_segments_mut()
        .map_err(|_| "Invalid S3 endpoint path")?
        .pop_if_empty()
        .push(bucket);
    url.set_query(Some("list-type=2&max-keys=1"));
    let host = match url.host().ok_or("Missing S3 endpoint host")? {
        url::Host::Ipv6(ip) => format!("[{ip}]"),
        host => host.to_string(),
    };
    let host = url
        .port()
        .map_or(host.clone(), |port| format!("{host}:{port}"));
    let region = create.s3_region.as_deref().ok_or("Missing S3 region")?;
    if region.is_empty()
        || !region
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        return Err("Invalid S3 signing region".into());
    }
    let access = create
        .s3_access_key_id
        .as_deref()
        .ok_or("Missing S3 access ID")?;
    if access.is_empty()
        || !access
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
    {
        return Err("Invalid S3 access ID".into());
    }
    let secret = create
        .s3_secret_access_key
        .as_deref()
        .ok_or("Missing S3 secret")?;
    let timestamp = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date = now.format("%Y%m%d").to_string();
    let payload = digest(b"");
    let mut headers = HeaderMap::new();
    headers.insert(HOST, header(&host, false)?);
    headers.insert("x-amz-content-sha256", header(&payload, false)?);
    headers.insert("x-amz-date", header(&timestamp, false)?);
    let mut canonical_headers =
        format!("host:{host}\nx-amz-content-sha256:{payload}\nx-amz-date:{timestamp}\n");
    let mut signed_headers = "host;x-amz-content-sha256;x-amz-date".to_string();
    if let Some(token) = create
        .s3_session_token
        .as_deref()
        .filter(|token| !token.trim().is_empty())
    {
        // Reject whitespace instead of signing a different normalized token.
        if token.is_empty() || token.bytes().any(|c| c.is_ascii_whitespace()) {
            return Err("Invalid S3 session token".into());
        }
        headers.insert("x-amz-security-token", header(token, true)?);
        canonical_headers.push_str(&format!("x-amz-security-token:{token}\n"));
        signed_headers.push_str(";x-amz-security-token");
    }
    let canonical = format!(
        "GET\n{}\n{}\n{canonical_headers}\n{signed_headers}\n{payload}",
        url.path(),
        url.query().ok_or("Missing S3 probe query")?
    );
    let scope = format!("{date}/{region}/s3/aws4_request");
    let to_sign = format!(
        "AWS4-HMAC-SHA256\n{timestamp}\n{scope}\n{}",
        digest(canonical)
    );
    let date_key = mac(format!("AWS4{secret}").as_bytes(), &date);
    let region_key = mac(&date_key, region);
    let service_key = mac(&region_key, "s3");
    let signing_key = mac(&service_key, "aws4_request");
    let signature = hex::encode(mac(&signing_key, &to_sign));
    headers.insert(AUTHORIZATION, header(&format!(
        "AWS4-HMAC-SHA256 Credential={access}/{scope}, SignedHeaders={signed_headers}, Signature={signature}"
    ), true)?);
    Ok((url, headers))
}

/// Checks bounded ListObjectsV2 bucket read access with pinned HTTPS transport.
///
/// # Errors
/// Returns a credential-free error for invalid local settings, rejected targets,
/// or connection failure. Remote status/body failures become safe stage results.
pub(super) async fn probe(
    create: &CreateCacheDestination,
    allow_private_targets: bool,
) -> Result<CacheCredentialTestResult, String> {
    let (url, headers) = signed_bucket_request(create, Utc::now())?;
    let client = cache_test_client(&url, allow_private_targets, None, None, None).await?;
    let response = client
        .get(url)
        .headers(headers)
        .send()
        .await
        .map_err(|_| "S3 bucket connection failed")?;
    let status = response.status();
    // Consume only a bounded body. Never return the server's error body, which
    // can reflect the access ID, session token, canonical request, or signature.
    let result = probe_body(response).await.and_then(|body| {
        let mut reader = quick_xml::Reader::from_reader(body.as_slice());
        loop {
            match reader.read_event() {
                Ok(quick_xml::events::Event::Start(root))
                | Ok(quick_xml::events::Event::Empty(root)) => {
                    return if root.local_name().as_ref() == b"ListBucketResult" {
                        Ok(())
                    } else {
                        Err("Invalid S3 ListObjectsV2 response".into())
                    };
                }
                Ok(quick_xml::events::Event::Eof) | Err(_) => {
                    return Err("Invalid S3 ListObjectsV2 response".into());
                }
                _ => {}
            }
        }
    });
    Ok(CacheCredentialTestResult {
        ok: result.is_ok(),
        status_code: Some(status.as_u16()),
        message: result.err().unwrap_or_else(|| {
            "S3 ListObjectsV2 read access successful; write authorization untested".into()
        }),
        tested_url: None,
        niks3: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sigv4_signs_exact_bucket_host_path_query_and_sensitive_session() {
        let create = CreateCacheDestination {
            push_to: Some("s3://fixture-bucket/prefix".into()),
            s3_endpoint_url: Some("https://objects.example:9443/reverse%20proxy/".into()),
            s3_region: Some("fixture-region".into()),
            s3_access_key_id: Some("fixture-access-marker".into()),
            s3_secret_access_key: Some("fixture-secret-marker".into()),
            s3_session_token: Some("fixture-session-marker".into()),
            ..Default::default()
        };
        let now = DateTime::parse_from_rfc3339("2026-10-04T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let (url, headers) = signed_bucket_request(&create, now).unwrap();
        assert_eq!(
            url.as_str(),
            "https://objects.example:9443/reverse%20proxy/fixture-bucket?list-type=2&max-keys=1"
        );
        assert_eq!(headers[HOST], "objects.example:9443");
        assert!(headers[AUTHORIZATION].is_sensitive());
        assert!(headers["x-amz-security-token"].is_sensitive());
        let auth = headers[AUTHORIZATION].to_str().unwrap();
        assert!(auth.contains("20261004/fixture-region/s3/aws4_request"));
        assert!(auth.contains("host;x-amz-content-sha256;x-amz-date;x-amz-security-token"));
        for (field, value) in [
            ("s3_secret_access_key", "replacement-secret-marker"),
            ("s3_region", "other-region"),
            ("s3_access_key_id", "replacement-access-marker"),
        ] {
            let mut json = serde_json::to_value(&create).unwrap();
            json[field] = value.into();
            let changed = serde_json::from_value(json).unwrap();
            assert_ne!(
                headers[AUTHORIZATION],
                signed_bucket_request(&changed, now).unwrap().1[AUTHORIZATION]
            );
        }
        for endpoint in [
            "https://user:fixture-secret@example",
            "https://example/?token=fixture-secret",
            "https://example/#fixture-secret",
        ] {
            let mut changed = create.clone();
            changed.s3_endpoint_url = Some(endpoint.into());
            let error = signed_bucket_request(&changed, now).unwrap_err();
            assert!(!error.contains("fixture-secret"));
        }
    }
}
