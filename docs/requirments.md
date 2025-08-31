# Crystal Forge Constraints & Policy

## Non-Negotiable Constraints

### Security & Memory Safety

- **Memory-safe implementation**: All components must be written in memory-safe languages (Rust primary)
- **Zero vulnerability introduction**: Crystal Forge cannot introduce security vulnerabilities to monitored systems
- **Minimal attack surface**: Agent runs with least privilege, minimal system access
- **Cryptographic verification**: All agent-server communication cryptographically signed (Ed25519)

### Data Handling & Privacy

- **Air-gapped capable**: Must operate without external internet dependencies
- **Data sovereignty**: All compliance data remains within organization boundaries
- **No telemetry**: No data transmission to external parties or vendors
- **Audit trail integrity**: Immutable logging of all compliance events and changes

### Platform Requirements

- **NixOS native**: Deep integration with Nix ecosystem, not just compatibility layer
- **Self-hosted only**: No SaaS or cloud dependencies for core functionality
- **PostgreSQL backend**: Proven, auditable database with strong ACID guarantees
- **Reproducible builds**: All components must build deterministically via Nix

### Regulatory Compliance

- **FIPS compatibility**: Support for FIPS-validated cryptographic modules
- **Common Criteria readiness**: Architecture supports future CC evaluation
- **Supply chain verification**: All dependencies trackable and verifiable
- **Compliance framework agnostic**: Support multiple frameworks (STIG, NIST, SOC2)

## Flexible Elements

- **Deployment scale**: Can adapt from single-system to enterprise-wide
- **Integration interfaces**: API extensible for third-party compliance tools
- **Reporting formats**: Multiple export formats for different audit requirements
- **Performance tuning**: Monitoring frequency and resource usage configurable

## Policy Requirements

- **Backward compatibility**: Configuration changes must not break existing deployments
- **Zero-downtime updates**: Agent and server updates without service interruption
- **Secure defaults**: All configuration defaults must be security-conscious
