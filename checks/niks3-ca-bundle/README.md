# Niks3 onboarding CA bundle regression

This focused sandbox check reproduces the public onboarding operation:
private issuing CA followed by `pkgs.cacert/etc/ssl/certs/ca-bundle.crt`.
The private issuing CA is replaced by an ephemeral synthetic P-256 root.
The check does not read operator files, call Vault, or start services.
OpenSSL generates the key in the build sandbox. The key is deleted immediately
after certificate generation and by an exit trap on failure. No key enters an
output. Only public certificates, synthetic certificates, counts, and digests
are retained. Certificate bytes are never printed in build logs.

The [CA compatibility operator contract](../../docs/knowledge/caches/niks3-ca-bundle-compatibility.md)
describes supported annotations, root-preserving normalization, replacement
trust for Niks3 writes, and the separate strict client-identity checks.

## Run

Snowfall discovers the new check without a root flake change. New files must
be in the Git index for Git-backed flake evaluation.

```sh
# Fixture/provenance measurement, independent of the production normalizer:
nix build --no-link -L .#checks.x86_64-linux.niks3-ca-bundle.fixture

# Production normalizer and independent Go certificate-identity comparison:
nix build --no-link -L .#checks.x86_64-linux.niks3-ca-bundle
```

The Rust runner is the small `cf-config` example `normalize_ca_bundle`.
It calls the actual public normalizer and writes output to a file. It has no
production package or runtime wiring. The runner build executes the existing
nine CA unit tests in its sandbox.

The Go comparator uses `pem.Decode`, skips non-`CERTIFICATE` blocks and blocks
with headers, and calls `x509.ParseCertificate`, matching
`CertPool.AppendCertsFromPEM`. It also calls the real pool implementation on
the input and each eligible certificate. Certificate identities are sorted,
deduplicated SHA-256 hashes of complete `Certificate.Raw` DER, not subjects.
The compatibility gate requires identical original and normalized identity
sets, exactly one added synthetic root, and certificate-only output with one
block per identity for this duplicate-free fixture. Every public ordinary and
trusted certificate is parsed by Go and OpenSSL. These checks establish
structural validity, not current validity dates or a live TLS connection.

## Public provenance

Only these two owner-source URLs were fetched on 2026-10-10 UTC:

- <https://gitlab.com/usmcamp0811/dotfiles/-/raw/nixos/packages/scripts/niks3-onboard/default.nix>
- <https://gitlab.com/usmcamp0811/dotfiles/-/raw/nixos/flake.lock>

The script accepts externally supplied `pkgs`. The public lock alone cannot
prove which caller package set created a deployed bundle. The check therefore
compares both root locked package sets with Crystal Forge's pin:

| Dataset | Lock node | nixpkgs revision |
| --- | --- | --- |
| Crystal Forge | `nixpkgs` | `3e3f3c7f9977dc123c23ee21e8085ed63daf8c37` |
| dotfiles root | `nixpkgs_76` | `36d0315f01000f976b9c37baf72910b1389983e4` |
| dotfiles unstable | `unstable_5` | `e7439b6b14ad3cc35d05608ebca9bce01a25f5f8` |

All three resolve `cacert` to `nss-cacert` 3.126 and the same NSS certdata:
`sha256-gbfyV2MzouNg5nP5Etewt6dl2DbHMQA+NIpGysXTcZg=`.
All three full bundles are byte-identical, with SHA-256
`3ba2000fe3d55cb6c41323fabbb105743e2c9422151eea213311538b07b23f5a`.
`fixture/provenance.json` retains the exact package and certdata store paths.
The external revisions are test-only fetches; repository pins and production
Niks3/Nix versions are unchanged.

## Measured structure and verification history

| Category | Public bundle | Synthetic onboarding bundle |
| --- | ---: | ---: |
| Lines | 8,226 | 8,238 |
| Ordinary certificates | 121 | 122 |
| Trusted certificates | 172 | 172 |
| Blank lines | 293 | 293 |
| CR-containing lines | 0 | 0 |
| Labels | 172 | 172 |
| Omission notes | 51 | 51 |
| Trusted headings | 170 | 170 |
| Rejected headings | 0 | 0 |
| OID lines | 212 | 212 |
| All PEM framing lines | 586 | 588 |
| Ordinary Base64 lines | 2,749 | 2,759 |
| Trusted Base64 lines | 3,993 | 3,993 |
| Private-key blocks | 0 | 0 |

The 605 annotation lines are `172 + 51 + 170 + 212`.
The ordinary-only parser reconstruction skips blank/comment lines and accepts
ordinary PEM framing/body lines. It rejects exactly
`605 + 344 trusted framing + 3993 trusted body = 4942` lines.
The synthetic root has 12 lines, so `8226 + 12 = 8238`, not 8266.
Root-dependent lengths are fixture measurements, not an attestation about an
operator certificate. Go selects 121 public identities plus one synthetic
identity. All 293 public X.509 objects parse successfully.

At initial verification, the fixture measurement and nine existing CA unit
tests passed. The compatibility gate failed with the static error
`ca_bundle_content_unsupported`. Exactly one public NSS label is
Base64-decodable; that version of `validate_label` rejected such labels.
The fixture preserved that label without sanitizing input or weakening the
identity-equivalence assertion.

The production parser now accepts a Base64-shaped NSS nickname only within
a complete validated paired record. The task owner reports a subsequent
passing compatibility run for all three datasets above. This is fixture
verification evidence, not inspection of an operator's private CA or key,
native TLS acceptance, or a diagnosis of an earlier production exit code 1.
Keep exact-source gate evidence with the fixture owner's verification record;
the documentation edit does not run or establish a new gate result.
