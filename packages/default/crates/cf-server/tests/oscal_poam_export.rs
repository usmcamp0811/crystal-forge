#[path = "../src/services/oscal_poam_export.rs"]
mod writer;

use chrono::{TimeZone, Utc};
use uuid::Uuid;
use writer::{Document, Entry, Source, write_poam};

#[test]
fn empty_selection_cannot_be_an_oscal_document() {
    let error = write_poam(&Document {
        uuid: Uuid::new_v4(),
        title: "Empty authorized scope",
        version: "1",
        last_modified: Utc::now(),
        entries: &[],
    })
    .err()
    .unwrap();
    assert!(error.to_string().contains("no items"));
}

fn validate(json: &str, xml: &str) {
    let dir = tempfile::tempdir().unwrap();
    let json_path = dir.path().join("plan.json");
    let xml_path = dir.path().join("plan.xml");
    let schemas =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../schemas/oscal-1.1.2");
    std::fs::write(&json_path, json).unwrap();
    std::fs::write(&xml_path, xml).unwrap();
    let json_check = std::process::Command::new("check-jsonschema")
        .arg("--schemafile")
        .arg(schemas.join("oscal_poam_schema.json"))
        .arg(&json_path)
        .output()
        .unwrap();
    assert!(
        json_check.status.success(),
        "{}{}",
        String::from_utf8_lossy(&json_check.stdout),
        String::from_utf8_lossy(&json_check.stderr)
    );
    let xml_check = std::process::Command::new("xmllint")
        .arg("--noout")
        .arg("--schema")
        .arg(schemas.join("oscal_poam_schema.xsd"))
        .arg(xml_path)
        .output()
        .unwrap();
    assert!(
        xml_check.status.success(),
        "{}",
        String::from_utf8_lossy(&xml_check.stderr)
    );
}

#[test]
fn all_source_items_are_valid_without_inferred_risk_or_approval() {
    let now = Utc
        .with_ymd_and_hms(2026, 9, 27, 12, 0, 0)
        .single()
        .unwrap();
    let scopes = vec![
        "system:11111111-1111-4111-8111-111111111111".into(),
        "environment:22222222-2222-4222-8222-222222222222".into(),
    ];
    let cves = vec!["CVE-2026-12345".into(), "CVE-2026-12346".into()];
    let entries = [
        Entry {
            uuid: Uuid::new_v4(),
            source_id: "POAM-7",
            title: "Patch service",
            description: "Replace & verify <service>.",
            scopes: &scopes,
            cve_ids: &cves,
            scheduled_cve_tuples: &[],
            cve_finding_links: &[],
            policy_finding_ids: &[Uuid::from_u128(42)],
            target_date: Some(now.date_naive()),
            technical_evidence: None,
            source: Source::Plan {
                status: "scheduled",
                created_at: now,
                updated_at: now,
                owner: Some("Operations"),
                assignee_user_id: Some(Uuid::from_u128(43)),
                assignee_group_name: None,
            },
        },
        Entry {
            uuid: Uuid::new_v4(),
            source_id: "cve_host:9",
            title: "Decision",
            description: "Reviewed exposure",
            scopes: &[],
            cve_ids: &[],
            scheduled_cve_tuples: &[],
            cve_finding_links: &[],
            policy_finding_ids: &[],
            target_date: None,
            technical_evidence: None,
            source: Source::Acceptance {
                human_id: Some("RA-0042"),
                source_kind: "cve-host-decision",
                rationale: "Decision & review",
                accepted_by: None,
                accepted_at: None,
                recorded_at: now,
                review_date: None,
                review_due_at: None,
                expires_at: None,
                retired_at: None,
                retired_by: None,
                retirement_reason: None,
                replacement_poam_id: None,
                status: "pending",
            },
        },
    ];
    let document = Document {
        uuid: Uuid::new_v4(),
        title: "Register export",
        version: "1",
        last_modified: now,
        entries: &entries,
    };
    let output = write_poam(&document).unwrap();
    let json: serde_json::Value = serde_json::from_str(&output.json).unwrap();
    let poam = &json["plan-of-action-and-milestones"];
    assert_eq!(poam["poam-items"].as_array().unwrap().len(), 2);
    assert!(poam.get("risks").is_none());
    assert!(poam.get("observations").is_none());
    assert!(poam.get("findings").is_none());
    let props = poam["poam-items"][0]["props"].as_array().unwrap();
    assert_eq!(props.iter().filter(|p| p["name"] == "cve-id").count(), 2);
    assert_eq!(props.iter().filter(|p| p["name"] == "scope").count(), 2);
    for (name, value) in [
        ("policy-finding-id", Uuid::from_u128(42).to_string()),
        ("plan-owner", "Operations".into()),
        ("plan-assignee-user-id", Uuid::from_u128(43).to_string()),
        ("plan-created-at", now.to_rfc3339()),
        ("plan-updated-at", now.to_rfc3339()),
    ] {
        assert!(
            props
                .iter()
                .any(|p| p["name"] == name && p["value"] == value)
        );
    }
    assert!(
        props
            .iter()
            .any(|p| p["name"] == "source-target-date" && p["value"] == "2026-09-27")
    );
    let decision = poam["poam-items"][1]["props"].as_array().unwrap();
    assert!(
        decision
            .iter()
            .any(|p| p["name"] == "source-id" && p["value"] == "cve_host:9")
    );
    assert!(
        decision
            .iter()
            .any(|p| p["name"] == "risk-acceptance-id" && p["value"] == "RA-0042")
    );
    assert!(
        output
            .xml
            .contains("name=\"risk-acceptance-id\" value=\"RA-0042\"")
    );
    assert!(decision.iter().any(|p| p["name"] == "decision-rationale"));
    assert!(!decision.iter().any(|p| p["name"] == "accepted-at"));
    assert!(!decision.iter().any(|p| p["name"] == "decision-expires-at"));
    assert!(!decision.iter().any(|p| p["name"] == "decision-retired-at"));
    assert!(
        !decision
            .iter()
            .any(|p| p["name"] == "decision-replacement-poam-id")
    );
    assert!(output.xml.contains("&amp; verify &lt;service&gt;"));
    validate(&output.json, &output.xml);
    for subset in [&entries[..1], &entries[1..]] {
        let output = write_poam(&Document {
            entries: subset,
            ..document
        })
        .unwrap();
        validate(&output.json, &output.xml);
    }
}

#[test]
fn rejects_partial_approval_without_emitting_a_document() {
    let entry = Entry {
        uuid: Uuid::new_v4(),
        source_id: "decision",
        title: "Decision",
        description: "Rationale",
        scopes: &[],
        cve_ids: &[],
        scheduled_cve_tuples: &[],
        cve_finding_links: &[],
        policy_finding_ids: &[],
        target_date: None,
        technical_evidence: None,
        source: Source::Acceptance {
            human_id: None,
            source_kind: "policy-waiver",
            rationale: "Rationale",
            accepted_by: Some(Uuid::new_v4()),
            accepted_at: None,
            recorded_at: Utc::now(),
            review_date: None,
            review_due_at: None,
            expires_at: None,
            retired_at: None,
            retired_by: None,
            retirement_reason: None,
            replacement_poam_id: None,
            status: "accepted",
        },
    };
    assert!(
        write_poam(&Document {
            uuid: Uuid::new_v4(),
            title: "Export",
            version: "1",
            last_modified: Utc::now(),
            entries: &[entry]
        })
        .unwrap_err()
        .to_string()
        .contains("incomplete approval")
    );
}
