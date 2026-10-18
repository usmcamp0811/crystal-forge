---
type: Operator Guide
title: Niks3 CA bundle compatibility and trust selection
description: Explains supported full NSS/OpenSSL CA bundles, Go-compatible root preservation, separate client identity validation, legacy normalization, and safe probe diagnostics.
tags:
  - crystal-forge
  - caches
  - niks3
  - tls
  - ca-bundle
implementation_status: implemented
---

# Niks3 CA bundle compatibility and trust selection

## Select complete server trust

The public onboarding recipe concatenates a private issuing CA and the full
`pkgs.cacert` OpenSSL/NSS bundle. That union is intentional: the private root
can authenticate the Niks3 API while public roots authenticate presigned HTTPS
object-storage targets. This guide does not inspect a deployed private CA or
its key. The [component dataset comparison](../../../packages/default/crates/cf-config/tests/ca-bundle/compatibility.nix)
defines the public recipe, pinned package inputs and provenance report. The
[component catalog](../testing/flake-checks.md#current-inventory-and-commands)
records execution ownership and the consolidation measurements.

Niks3's Go client uses one custom root pool for both API and presigned S3
connections. A selected custom CA replaces system roots; it does not augment
them. Include every required API/storage server root and the public roots when
public object storage needs them. A private issuing CA alone can therefore
be insufficient even when the API succeeds. Client-certificate issuer trust
is a separate server-side policy, not a substitute for server trust.

The Crystal Forge Write Test and discovery client use the same replacement
policy when a custom write CA is selected. They disable built-in roots and do
not borrow `SSL_CERT_FILE`, `NIX_SSL_CERT_FILE`, or other process roots.
Without a custom CA, the probe uses its normal system/process trust path.
Read transport retains its existing plane-specific policy; this correction
does not change Basic authority guards, mTLS read authorization, signatures,
or credential delivery.

## Supported input and root-preserving normalization

`cf_config::ca_bundle::normalize_ca_bundle` accepts ordinary `CERTIFICATE`
PEM and supported buildcatrust v0.5.1 OpenSSL records. It produces ordinary
certificate-only PEM with LF endings and 64-character Base64 lines. It
preserves ordinary certificate order and duplicates. It adds no ambient roots.

A buildcatrust record requires a bounded label, a plain certificate or the
exact omission note, optional exact `Trusted for:` / `Rejected for:` purpose
lists, and a matching `TRUSTED CERTIFICATE` with validated AUX DER. The
omission note is:

```text
Traditional PEM block omitted: this certificate is not trusted for authenticating servers.
```

Purpose lists use the generator's exact indentation, OIDs, and descriptions
for the eight supported purposes. The parser compares those lists with AUX
trust/reject metadata. It checks complete DER consumption, matching plain and
trusted certificate identities, and server-authentication selection. A
serverAuth rejection cannot be hidden by another ordinary copy of the same
certificate elsewhere in the input.

Standalone `TRUSTED CERTIFICATE` blocks are also parsed and validated, then
excluded from output. They are never relabeled as ordinary roots. The same
rule applies to trusted-only objects inside paired records. This preserves
the identities selected by Go `CertPool.AppendCertsFromPEM`; it is not a
general OpenSSL trust-store conversion.

The parser permits whitespace and complete `#` comment lines outside PEM
bodies, subject to checks that reject PEM markers, private-key headers, and
encryption headers in those comments. Arbitrary unrecognized text is not
discarded. Labels are constrained to complete validated records, not matched
to a subject CN. A Base64-shaped NSS nickname is accepted only in that context.
Inline Base64 comments, unsupported AUX aliases/key IDs/other algorithms,
unknown purposes, encrypted/private objects, and contradictory trust fail
closed. A trusted-only bundle with no ordinary roots fails as empty.

Parser bounds are 4 MiB per input, 4096 bytes per line, 128 KiB of encoded
Base64 per block, 8192 records, and 256 characters per label. Valid X.509
structure does not establish current validity dates, hostname coverage,
remote authorization, or ownership of a private key. TLS still performs its
normal certificate verification.

## Save, legacy projection, and identity boundaries

The shared normalizer applies before Save and when producing current GET,
Test, dispatch, SDK, and protected CLI CA-file payloads. Save writes canonical
CA PEM. Recognized legacy annotations are normalized in memory on GET/Test;
those reads do not rewrite rows, ciphertext, timestamps, or assignments.
A CA-only Save preserves omitted encrypted token/private-key fields through
the existing effective-update merge. Secret replacement and explicit clears
retain their existing rules.

Client certificate fields remain strict certificate-only chains with the leaf
first. They do not accept CA annotations, AUX trusted blocks, or comments.
Save's general certificate checks do not prove key pairing; the consuming CLI
still validates its actual identity and authorization.

Write Test and discovery additionally validate the client identity locally
before DNS. They parse the strict chain and exactly one supported unencrypted
private-key PEM block, then compare the key with the first certificate's
public key using `public_eq`. PKCS#8, RSA, and EC framing is supported only
when the cryptographic parser and actual probe TLS backend accept the key.
Unsupported algorithms, encrypted keys, malformed material, and mismatches
return static errors. The check does not prove dates, issuer authorization,
certificate EKUs, or upload permission. Private keys never enter responses.

## Exact probe diagnostic codes

The server and browser use these 17 exact allowlisted codes. Messages are
static local guidance, not raw provider or cryptographic-parser text.

| Code | Operator interpretation |
| --- | --- |
| `ca_bundle_framing_invalid` | Check PEM framing and encoding. |
| `ca_certificate_invalid` | A CA certificate is not valid X.509 structure. |
| `ca_bundle_content_unsupported` | Input is outside the supported grammar; private keys are prohibited. |
| `ca_bundle_trust_ambiguous` | Trust annotations contradict or do not match certificate selection. |
| `ca_bundle_empty` | No ordinary roots remain for the custom pool. |
| `ca_bundle_too_large` | A parser resource bound was exceeded. |
| `client_identity_invalid` | Check the complete selected client identity. |
| `client_certificate_invalid` | Check the certificate-only leaf-first chain. |
| `client_private_key_invalid` | Check the supported unencrypted single-key input. |
| `client_identity_mismatch` | The key does not match the first certificate. |
| `tls_verification_failed` | Check server trust, dates, hostname, and selected identity. |
| `dns_or_connection_failed` | Check DNS, routing, and endpoint availability. |
| `http_response_failed` | The endpoint returned an unsuccessful response or redirect. |
| `invalid_niks3_metadata` | Metadata is invalid or inconsistent. |
| `target_policy_rejected` | The target is outside the permitted probe policy. |
| `probe_timeout` | The probe reached its deadline. |
| `probe_failed` | No more specific safe classification is available. |

Responses use `diagnostic_code` and, for applicable local failures, a static
`field`; error `details` remains null. Unknown/decorated codes and untrusted
response prose have no display authority. The browser uses generic local
guidance rather than echoing URLs, bodies, PEM, or transport exceptions.
Existing authorization/CSRF and target, pinned-DNS, verified-TLS, no-proxy,
no-redirect, timeout, and body-bound rules remain enforced.

A green metadata result reports only the checked API/read stage. Public
cache-config cannot establish write authentication or authorization:
`write_auth_valid`, `write_authn_valid`, and `write_authorization_valid` remain
null when unproved and display **Untested**. Confirm publication with a real
job and the independent server exact-output metadata probe. Completion uses
`path-info --refresh --store`, with a 60-second metadata deadline; it does not
copy the closure or prove signatures or NAR integrity. Configured public keys
and `require-sigs` remain settings, not full signature proof from `path-info`.
Deployment, CVE materialization, and normal Nix substitution keep their
copy-time signature and NAR-hash checks. Separate native fresh-store fixture
reads provide artifact-validation coverage; metadata confirmation does not
replace that coverage. See [publication recovery](niks3-cache.md#publication-failure-and-recovery)
for guarded confirmed/pending completion and bounded callback retries.

## Proof boundaries and diagnosis

The focused Go/Rust reproduction uses one synthetic private issuer plus the
full public NSS bundle. It compares complete DER-identity sets selected by Go,
not subject names. The component comparison records the three package origins
in its generated provenance report. Normal core tests explicitly select all
thirteen unchanged `cf-config` library tests; `--package cf-server` alone did
not select dependency-library tests. Core `postCheck` owns the separate real
Go/Rust dataset comparison, with no public verification package, CA runner or
fixture derivation. Go invocations decrease from nine to three and OpenSSL
parses from 1,758 to 879; complete identity equality remains required.
The public bundle contains 121 ordinary and 172 trusted certificates, plus
605 annotation lines. The annotations are 172 labels, 51 omission notes,
170 trust headings, and 212 OID lines. They are supported generator structure,
not evidence of malformed certificates. The old ordinary-only reconstruction's
4942 unexpected lines consist of those 605 annotations, 344 trusted framing
lines, and 3993 trusted Base64 lines. That number describes parser limitations.

The separate [historical native integration fixture](https://gitlab.com/crystal-forge/crystal-forge/-/blob/5fa1f4fa/checks/niks3-cache/README.md)
used three synthetic private roots plus 121 public ordinary roots. TASK-490.24
retires that suite before replacement coverage is complete. The following
describes its historical proof boundaries, not an executed Delivery replacement.
Its Save,
GET, and CLI-file checks compare the complete canonical 124-root bundle while
validating and excluding 172 trusted blocks. Zero trusted-only roots may be
promoted. The fixture also distinguishes ambient trust from selected custom
trust: an ambient-root success cannot rescue a B-only custom pool missing API
root A. Fresh NAR/narinfo writes and signature-required reads are separate from
metadata Test. These fixtures do not inspect a deployed private identity or
prove its production TLS configuration.

Use the full supported CA bundle required by both write endpoints. Do not
silently strip unsupported records or convert every trusted AUX certificate
into an ordinary root. A source that intentionally provides ordinary-only PEM
can be used when it supplies the required roots; changing the public onboarding
script to such a source is not required by this compatibility contract.
Keep certificate/private-key material out of diagnosis output.

The [operator guide](niks3-cache.md) describes bounded CLI diagnostic hints.
An earlier deployment's exit code 1 alone does not identify TLS trust, client
identity, or storage failure. The original cause remains unknown without safe
evidence from that operation. Current capture improves future diagnosis; it
does not retrospectively prove a cause or a running upstream version.
