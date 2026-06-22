//! Deterministic OSCAL 1.1.2 Assessment Results fixture generator.
//!
//! Duplicates `build_oscal()` from `packages/web-ui/src/export/mod.rs` using
//! deterministic UUIDs, timestamps, and fixture data so that CI can validate
//! the output against the official NIST JSON schemas.
//!
//! Every UUID is derived from a fixed seed via `Uuid::from_u128()` to make
//! the output byte-identical across runs.  The implementation must be kept in
//! sync with the web-ui export module.

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use uuid::Uuid;

// ─── Constants ────────────────────────────────────────────────────────────────

const CF_NS: &str = "https://crystal-forge.example/ns/oscal";
const NOW_ISO: &str = "2026-06-21T12:00:00Z";

/// Deterministic UUIDs derived from incrementing seeds.
const fn det_uuid(seed: u128) -> Uuid {
    Uuid::from_u128(seed)
}

// ─── Data types (mirrors web-ui `ExportPayload` and friends) ──────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ComplianceControlStatus {
    Pass,
    Warn,
    Fail,
    Waiver,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ComplianceBundleSummary {
    id: Uuid,
    name: String,
    framework: String,
    version: String,
    layer: String,
    owner: String,
    description: Option<String>,
    last_review: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ComplianceRollupTotals {
    system_count: i64,
    fully_compliant_count: i64,
    overall_score: i64,
    pass: i64,
    warn: i64,
    fail: i64,
    waiver: i64,
    total_controls: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ComplianceSystemRollup {
    system_id: Uuid,
    hostname: String,
    environment: Option<String>,
    score: i64,
    pass: i64,
    warn: i64,
    fail: i64,
    waiver: i64,
    total: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ComplianceEvidenceResponse {
    bundle_id: Uuid,
    system_id: Uuid,
    hostname: String,
    controls: Vec<ComplianceControlEvidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ComplianceControlEvidence {
    policy_id: Uuid,
    policy_name: String,
    status: ComplianceControlStatus,
    severity: String,
    summary: String,
    evidence_items: Vec<ComplianceEvidenceItem>,
    framework_mapping: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ComplianceEvidenceItem {
    kind: String,
    label: String,
    body: String,
    artifact: Option<ComplianceEvidenceArtifact>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ComplianceEvidenceArtifact {
    artifact_type: String,
    title: String,
    body: String,
}

struct ExportPayload<'a> {
    bundle: &'a ComplianceBundleSummary,
    totals: &'a ComplianceRollupTotals,
    systems: &'a [ComplianceSystemRollup],
    evidence: &'a [ComplianceEvidenceResponse],
    include_waivers: bool,
    include_source: bool,
    scope: &'a str,
}

impl<'a> ExportPayload<'a> {
    fn scoped_systems(&self) -> Vec<&ComplianceSystemRollup> {
        self.systems
            .iter()
            .filter(|s| match self.scope {
                "fail" => s.fail > 0,
                "clean" => s.fail == 0 && s.warn == 0,
                _ => true,
            })
            .collect()
    }

    fn scoped_evidence(&self) -> Vec<&ComplianceEvidenceResponse> {
        let scoped_ids: HashSet<Uuid> =
            self.scoped_systems().iter().map(|s| s.system_id).collect();
        self.evidence
            .iter()
            .filter(|e| scoped_ids.contains(&e.system_id))
            .collect()
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn slugify_for_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '.' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn objective_id_for(policy_id: Uuid, policy_name: &str) -> String {
    let slug: String = policy_name
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .filter(|c| *c != '\0')
        .collect();
    let mut collapsed = String::with_capacity(slug.len());
    let mut prev_hyphen = false;
    for c in slug.chars() {
        if c == '-' {
            if prev_hyphen { continue; }
            prev_hyphen = true;
        } else {
            prev_hyphen = false;
        }
        collapsed.push(c);
    }
    let slug = collapsed.trim_matches('-').to_string();
    let short_id = policy_id.simple().to_string();
    format!("cf-obj-{}-{}", slug, &short_id[..8])
}

// ─── OSCAL builder (mirrors web-ui build_oscal) ───────────────────────────────

fn build_oscal(p: &ExportPayload<'_>) -> String {
    let ar_uuid = Uuid::from_u128(100).to_string();
    let ap_uuid = Uuid::from_u128(200).to_string();

    let scoped_ev = p.scoped_evidence();

    // Unique policies for objective definitions + include-objectives
    let unique_policies: Vec<&ComplianceControlEvidence> = {
        let mut seen = HashSet::new();
        let mut policies = Vec::new();
        for ev in &scoped_ev {
            for ctrl in &ev.controls {
                if !p.include_waivers && matches!(ctrl.status, ComplianceControlStatus::Waiver) {
                    continue;
                }
                if seen.insert(ctrl.policy_id) {
                    policies.push(ctrl);
                }
            }
        }
        policies
    };

    let include_objectives_list: Vec<Value> = unique_policies
        .iter()
        .map(|ctrl| json!({"objective-id": objective_id_for(ctrl.policy_id, &ctrl.policy_name)}))
        .collect();

    let objectives: Vec<Value> = unique_policies
        .iter()
        .map(|ctrl| {
            json!({
                "id": objective_id_for(ctrl.policy_id, &ctrl.policy_name),
                "title": ctrl.policy_name,
                "description": ctrl.summary,
                "props": [{
                    "name": "policy-uuid",
                    "ns": CF_NS,
                    "value": ctrl.policy_id.to_string(),
                }, {
                    "name": "framework-mapping",
                    "ns": CF_NS,
                    "value": ctrl.framework_mapping,
                }]
            })
        })
        .collect();

    // Components = one per host
    let components: Vec<Value> = p
        .scoped_systems()
        .iter()
        .map(|s| {
            json!({
                "uuid": s.system_id.to_string(),
                "type": "hardware",
                "title": s.hostname,
                "description": format!("Host {} in environment {}",
                    s.hostname, s.environment.as_deref().unwrap_or("unknown")),
                "status": { "state": if s.fail == 0 { "operational" } else { "under-development" } }
            })
        })
        .collect();

    // Clone for embedded documents
    let objectives_ap = objectives.clone();
    let include_objectives_ap = include_objectives_list.clone();
    let components_ssp = components.clone();

    // SSP
    let ssp_uuid = Uuid::from_u128(300).to_string();
    let ssp_json = json!({
        "system-security-plan": {
            "uuid": ssp_uuid,
            "metadata": {
                "title": format!("System Security Plan for {}", p.bundle.name),
                "last-modified": NOW_ISO,
                "version": "1.0",
                "oscal-version": "1.1.2",
                "props": [{
                    "name": "classification",
                    "ns": CF_NS,
                    "value": "UNCLASSIFIED"
                }],
                "parties": [{
                    "uuid": Uuid::from_u128(301).to_string(),
                    "type": "organization",
                    "name": "Crystal Forge",
                }]
            },
            "import-profile": {
                "href": "#generated-profile",
                "remarks": "Minimal SSP auto-generated by Crystal Forge export."
            },
            "system-characteristics": {
                "system-name": p.bundle.name.clone(),
                "system-id": p.bundle.id.to_string(),
                "security-sensitivity-level": "low",
                "description": p.bundle.description.clone().unwrap_or_default(),
            },
            "system-implementation": {
                "components": components_ssp,
            }
        }
    });
    let ssp_base64 = BASE64.encode(
        serde_json::to_string_pretty(&ssp_json).unwrap_or_default().as_bytes()
    );

    // AP
    let ap_json = json!({
        "assessment-plan": {
            "uuid": ap_uuid,
            "metadata": {
                "title": format!("Assessment Plan for {}", p.bundle.name),
                "last-modified": NOW_ISO,
                "version": "1.0",
                "oscal-version": "1.1.2",
                "props": [{
                    "name": "classification",
                    "ns": CF_NS,
                    "value": "UNCLASSIFIED"
                }],
                "parties": [{
                    "uuid": Uuid::from_u128(201).to_string(),
                    "type": "organization",
                    "name": "Crystal Forge",
                }]
            },
            "import-ssp": {
                "href": format!("#{}", ssp_uuid),
            },
            "local-definitions": {
                "objectives-and-methods": {
                    "objectives": objectives_ap,
                }
            },
            "reviewed-controls": {
                "control-objective-selections": [{
                    "description": format!("Control objectives assessed for bundle '{}'", p.bundle.name),
                    "include-objectives": include_objectives_ap,
                }]
            }
        }
    });
    let ap_base64 = BASE64.encode(
        serde_json::to_string_pretty(&ap_json).unwrap_or_default().as_bytes()
    );

    // Observations + Findings
    let mut observations: Vec<Value> = Vec::new();
    let mut findings: Vec<Value> = Vec::new();

    for ev in &scoped_ev {
        let rollup = p.systems.iter().find(|s| s.system_id == ev.system_id);
        for ctrl in &ev.controls {
            if !p.include_waivers && matches!(ctrl.status, ComplianceControlStatus::Waiver) {
                continue;
            }
            let obs_seed: u128 = 1000
                + (ev.system_id.as_u128() % 100) * 10
                + (ctrl.policy_id.as_u128() % 10);
            let finding_seed: u128 = 2000
                + (ev.system_id.as_u128() % 100) * 10
                + (ctrl.policy_id.as_u128() % 10);
            let obs_uuid = Uuid::from_u128(obs_seed).to_string();
            let finding_uuid = Uuid::from_u128(finding_seed).to_string();

            let (oscal_state, oscal_reason) = match ctrl.status {
                ComplianceControlStatus::Pass => ("satisfied", "pass"),
                ComplianceControlStatus::Warn => ("not-satisfied", "other"),
                ComplianceControlStatus::Fail => ("not-satisfied", "fail-adjusted"),
                ComplianceControlStatus::Waiver => ("not-satisfied", "accept-risk"),
            };

            let relevant_evidence: Vec<Value> = if p.include_source {
                ctrl.evidence_items.iter().map(|item| {
                    json!({"description": format!("{}: {}", item.label, item.body)})
                }).collect()
            } else {
                vec![]
            };

            let is_disabled = ctrl.evidence_items
                .iter()
                .any(|item| item.body.contains("enabled=false"));

            let mut obs_props = vec![
                json!({"name": "framework-mapping", "ns": CF_NS, "value": ctrl.framework_mapping}),
                json!({"name": "severity", "ns": CF_NS, "value": ctrl.severity}),
                json!({"name": "execution-mode", "ns": CF_NS, "value": "automated"}),
            ];
            if is_disabled {
                obs_props.push(json!({"name": "evaluation-status", "ns": CF_NS, "value": "not-evaluated"}));
                obs_props.push(json!({"name": "policy-enabled", "ns": CF_NS, "value": "false"}));
            } else {
                obs_props.push(json!({"name": "evaluation-status", "ns": CF_NS, "value": "evaluated"}));
                obs_props.push(json!({"name": "policy-enabled", "ns": CF_NS, "value": "true"}));
            }
            if matches!(ctrl.status, ComplianceControlStatus::Waiver) {
                obs_props.push(json!({"name": "disposition", "ns": CF_NS, "value": "waived"}));
            }

            observations.push(json!({
                "uuid": obs_uuid,
                "title": format!("{} — {}", ev.hostname, ctrl.policy_name),
                "description": ctrl.summary,
                "methods": ["TEST"],
                "types": ["finding"],
                "subjects": [{
                    "subject-uuid": ev.system_id.to_string(),
                    "type": "component",
                    "title": ev.hostname,
                }],
                "relevant-evidence": relevant_evidence,
                "collected": NOW_ISO,
                "props": obs_props,
            }));

            let objective_id = objective_id_for(ctrl.policy_id, &ctrl.policy_name);
            let mut finding_props = vec![
                json!({"name": "hostname", "ns": CF_NS, "value": ev.hostname}),
                json!({"name": "environment", "ns": CF_NS,
                    "value": rollup.and_then(|r| r.environment.as_deref()).unwrap_or("unknown")}),
                json!({"name": "score", "ns": CF_NS,
                    "value": rollup.map(|r| r.score.to_string()).unwrap_or_default()}),
                json!({"name": "evaluation-status", "ns": CF_NS,
                    "value": if is_disabled { "not-evaluated" } else { "evaluated" }}),
                json!({"name": "policy-enabled", "ns": CF_NS,
                    "value": if is_disabled { "false" } else { "true" }}),
            ];
            if matches!(ctrl.status, ComplianceControlStatus::Waiver) {
                finding_props.push(json!({"name": "disposition", "ns": CF_NS, "value": "waived"}));
            }

            findings.push(json!({
                "uuid": finding_uuid,
                "title": format!("{} on {}", ctrl.policy_name, ev.hostname),
                "description": ctrl.summary,
                "target": {
                    "type": "objective-id",
                    "target-id": objective_id,
                    "title": ctrl.policy_name,
                    "status": { "state": oscal_state, "reason": oscal_reason }
                },
                "related-observations": [{ "observation-uuid": obs_uuid }],
                "props": finding_props,
            }));
        }
    }

    let doc = json!({
        "assessment-results": {
            "uuid": ar_uuid,
            "metadata": {
                "title": format!("{} Assessment Results", p.bundle.name),
                "last-modified": NOW_ISO,
                "version": "1.0",
                "oscal-version": "1.1.2",
                "props": [{
                    "name": "classification",
                    "ns": CF_NS,
                    "value": "UNCLASSIFIED"
                }],
                "parties": [{
                    "uuid": Uuid::from_u128(101).to_string(),
                    "type": "organization",
                    "name": "Crystal Forge",
                }]
            },
            "import-ap": {
                "href": format!("#{}", ap_uuid),
            },
            "local-definitions": {
                "components": components,
                "objectives-and-methods": {
                    "objectives": objectives,
                }
            },
            "back-matter": {
                "resources": [
                    {
                        "uuid": ap_uuid,
                        "title": format!("Assessment Plan for {}", p.bundle.name),
                        "description": "Embedded minimal OSCAL Assessment Plan with import-ssp, reviewed-controls, and local objectives.",
                        "base64": {
                            "filename": format!("{}-assessment-plan.json",
                                slugify_for_filename(&p.bundle.name)),
                            "media-type": "application/oscal+json",
                            "value": ap_base64,
                        }
                    },
                    {
                        "uuid": ssp_uuid,
                        "title": format!("System Security Plan for {}", p.bundle.name),
                        "description": "Embedded minimal OSCAL SSP referenced by the AP's import-ssp.",
                        "base64": {
                            "filename": format!("{}-system-security-plan.json",
                                slugify_for_filename(&p.bundle.name)),
                            "media-type": "application/oscal+json",
                            "value": ssp_base64,
                        }
                    }
                ]
            },
            "results": [{
                "uuid": Uuid::from_u128(102).to_string(),
                "title": format!("{} v{} Assessment", p.bundle.name, p.bundle.version),
                "description": p.bundle.description.as_deref().unwrap_or(""),
                "start": NOW_ISO,
                "end": NOW_ISO,
                "props": [{
                    "name": "overall-score", "ns": CF_NS, "value": p.totals.overall_score.to_string(),
                }, {
                    "name": "framework", "ns": CF_NS, "value": p.bundle.framework.clone(),
                }, {
                    "name": "compliant-hosts", "ns": CF_NS,
                    "value": format!("{} of {}", p.totals.fully_compliant_count, p.totals.system_count),
                }],
                "reviewed-controls": {
                    "description": format!("{} control objectives reviewed", p.totals.total_controls),
                    "control-objective-selections": [{
                        "description": format!("Control objectives assessed for bundle '{}'", p.bundle.name),
                        "include-objectives": include_objectives_list,
                    }]
                },
                "observations": observations,
                "findings": findings,
            }]
        }
    });

    serde_json::to_string_pretty(&doc).unwrap_or_default()
}

// ─── Fixture data ─────────────────────────────────────────────────────────────

fn build_fixture() -> String {
    let bundle = ComplianceBundleSummary {
        id: det_uuid(1),
        name: "Test Bundle".into(),
        framework: "NIST 800-53".into(),
        version: "1.0.0".into(),
        layer: "System".into(),
        owner: "admin".into(),
        description: Some("Test compliance bundle for OSCAL export validation.".into()),
        last_review: Some("2026-06-01".into()),
    };

    let sys1_id = det_uuid(10);
    let sys2_id = det_uuid(11);

    let systems = vec![
        ComplianceSystemRollup {
            system_id: sys1_id,
            hostname: "server-01".into(),
            environment: Some("prod".into()),
            score: 67,
            pass: 2,
            warn: 0,
            fail: 1,
            waiver: 0,
            total: 3,
        },
        ComplianceSystemRollup {
            system_id: sys2_id,
            hostname: "server-02".into(),
            environment: Some("staging".into()),
            score: 100,
            pass: 3,
            warn: 0,
            fail: 0,
            waiver: 0,
            total: 3,
        },
    ];

    let pol1_id = det_uuid(20);
    let pol2_id = det_uuid(21);
    let pol3_id = det_uuid(22);

    let evidence = vec![
        ComplianceEvidenceResponse {
            bundle_id: bundle.id,
            system_id: sys1_id,
            hostname: "server-01".into(),
            controls: vec![
                ComplianceControlEvidence {
                    policy_id: pol1_id,
                    policy_name: "require_ssh_key_auth".into(),
                    status: ComplianceControlStatus::Pass,
                    severity: "high".into(),
                    summary: "server-01 satisfies require_ssh_key_auth from available Crystal Forge data.".into(),
                    evidence_items: vec![ComplianceEvidenceItem {
                        kind: "policy_eval".into(),
                        label: "SSH key auth policy".into(),
                        body: "policy_type=require_ssh_key_auth enabled=true health_status=healthy critical_cves=0 high_cves=0".into(),
                        artifact: Some(ComplianceEvidenceArtifact {
                            artifact_type: "policy_eval".into(),
                            title: "Authoritative Crystal Forge signal".into(),
                            body: "policy_type=require_ssh_key_auth enabled=true health_status=healthy critical_cves=0 high_cves=0".into(),
                        }),
                    }],
                    framework_mapping: "NIST 800-53 → AC-2".into(),
                },
                ComplianceControlEvidence {
                    policy_id: pol2_id,
                    policy_name: "require_no_critical_cves".into(),
                    status: ComplianceControlStatus::Fail,
                    severity: "high".into(),
                    summary: "server-01 violates require_no_critical_cves according to current Crystal Forge data.".into(),
                    evidence_items: vec![ComplianceEvidenceItem {
                        kind: "policy_eval".into(),
                        label: "CVE check policy".into(),
                        body: "policy_type=require_cve_check enabled=true health_status=healthy critical_cves=1 high_cves=2".into(),
                        artifact: Some(ComplianceEvidenceArtifact {
                            artifact_type: "cve_scan".into(),
                            title: "Authoritative Crystal Forge signal".into(),
                            body: "policy_type=require_cve_check enabled=true health_status=healthy critical_cves=1 high_cves=2".into(),
                        }),
                    }],
                    framework_mapping: "NIST 800-53 → SI-2".into(),
                },
                ComplianceControlEvidence {
                    policy_id: pol3_id,
                    policy_name: "require_packages".into(),
                    status: ComplianceControlStatus::Pass,
                    severity: "medium".into(),
                    summary: "server-01 satisfies require_packages from available Crystal Forge data.".into(),
                    evidence_items: vec![ComplianceEvidenceItem {
                        kind: "policy_eval".into(),
                        label: "Package policy".into(),
                        body: "policy_type=require_packages enabled=true health_status=healthy critical_cves=0 high_cves=0".into(),
                        artifact: Some(ComplianceEvidenceArtifact {
                            artifact_type: "policy_eval".into(),
                            title: "Authoritative Crystal Forge signal".into(),
                            body: "policy_type=require_packages enabled=true health_status=healthy critical_cves=0 high_cves=0".into(),
                        }),
                    }],
                    framework_mapping: "NIST 800-53 → CM-6".into(),
                },
            ],
        },
        ComplianceEvidenceResponse {
            bundle_id: bundle.id,
            system_id: sys2_id,
            hostname: "server-02".into(),
            controls: vec![
                ComplianceControlEvidence {
                    policy_id: pol1_id,
                    policy_name: "require_ssh_key_auth".into(),
                    status: ComplianceControlStatus::Pass,
                    severity: "high".into(),
                    summary: "server-02 satisfies require_ssh_key_auth from available Crystal Forge data.".into(),
                    evidence_items: vec![ComplianceEvidenceItem {
                        kind: "policy_eval".into(),
                        label: "SSH key auth policy".into(),
                        body: "policy_type=require_ssh_key_auth enabled=true health_status=healthy critical_cves=0 high_cves=0".into(),
                        artifact: Some(ComplianceEvidenceArtifact {
                            artifact_type: "policy_eval".into(),
                            title: "Authoritative Crystal Forge signal".into(),
                            body: "policy_type=require_ssh_key_auth enabled=true health_status=healthy critical_cves=0 high_cves=0".into(),
                        }),
                    }],
                    framework_mapping: "NIST 800-53 → AC-2".into(),
                },
                ComplianceControlEvidence {
                    policy_id: pol2_id,
                    policy_name: "require_no_critical_cves".into(),
                    status: ComplianceControlStatus::Pass,
                    severity: "high".into(),
                    summary: "server-02 satisfies require_no_critical_cves from available Crystal Forge data.".into(),
                    evidence_items: vec![ComplianceEvidenceItem {
                        kind: "policy_eval".into(),
                        label: "CVE check policy".into(),
                        body: "policy_type=require_cve_check enabled=true health_status=healthy critical_cves=0 high_cves=0".into(),
                        artifact: Some(ComplianceEvidenceArtifact {
                            artifact_type: "cve_scan".into(),
                            title: "Authoritative Crystal Forge signal".into(),
                            body: "policy_type=require_cve_check enabled=true health_status=healthy critical_cves=0 high_cves=0".into(),
                        }),
                    }],
                    framework_mapping: "NIST 800-53 → SI-2".into(),
                },
                ComplianceControlEvidence {
                    policy_id: pol3_id,
                    policy_name: "require_packages".into(),
                    status: ComplianceControlStatus::Pass,
                    severity: "medium".into(),
                    summary: "server-02 satisfies require_packages from available Crystal Forge data.".into(),
                    evidence_items: vec![ComplianceEvidenceItem {
                        kind: "policy_eval".into(),
                        label: "Package policy".into(),
                        body: "policy_type=require_packages enabled=true health_status=healthy critical_cves=0 high_cves=0".into(),
                        artifact: Some(ComplianceEvidenceArtifact {
                            artifact_type: "policy_eval".into(),
                            title: "Authoritative Crystal Forge signal".into(),
                            body: "policy_type=require_packages enabled=true health_status=healthy critical_cves=0 high_cves=0".into(),
                        }),
                    }],
                    framework_mapping: "NIST 800-53 → CM-6".into(),
                },
            ],
        },
    ];

    let totals = ComplianceRollupTotals {
        system_count: 2,
        fully_compliant_count: 1,
        overall_score: 83,
        pass: 5,
        warn: 0,
        fail: 1,
        waiver: 0,
        total_controls: 3,
    };

    let payload = ExportPayload {
        bundle: &bundle,
        totals: &totals,
        systems: &systems,
        evidence: &evidence,
        include_waivers: true,
        include_source: false,
        scope: "all",
    };

    build_oscal(&payload)
}

// ─── Entry point ──────────────────────────────────────────────────────────────

fn main() {
    let output = build_fixture();
    println!("{output}");
}
