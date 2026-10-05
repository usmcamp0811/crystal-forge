# Design Specification

* [CF-XCCDF Interchange Profile (retained design handoff)](cf-xccdf-interchange-profile.md) - Points to the retained v0.1 draft that defines how Crystal Forge imports and exports compliance bundles and policies as XCCDF 1.2 XML with a Crystal Forge extension, including conformance classes, round-trip, trust, and open decisions.
* [Compliance Implementation Roadmap (retained Backlog document)](compliance-implementation-roadmap.md) - Points to the retained Backlog document doc-12 that sequences compliance work in phases (policy bridge, MVP domain and evaluator, backend-backed UX and interop, final design parity) with readiness gates and overlap decisions.

# Operator Guide

* [CF-XCCDF Compliance Interchange Operator Guide](cf-xccdf-interchange-operator-guide.md) - Explains operating CF-XCCDF compliance interchange: bundle and policy version lineage, importing foreign STIG/XCCDF and CF-XCCDF, trust and publication, XCCDF export, policy JSON/TOML interchange, and the tested compatibility limits.
* [Compliance Assignments, Overlays, and Report-Only Enforcement](assignments-and-report-only-enforcement.md) - Explains how a compliance bundle assignment resolves its effective policy set (exclusions, additions, overrides, system over environment), the enforce versus report_only modes, composite assessments, and how report-only failures affect blocking, waivers, and POA&Ms.
* [Crystal Forge STIG Module System](stig-modules.md) - Explains the mkStigModule factory, per-control enable and mandatory-justification options, active and inactive control tracking, how to add a control, and how a downstream flake imports and configures STIG controls.
