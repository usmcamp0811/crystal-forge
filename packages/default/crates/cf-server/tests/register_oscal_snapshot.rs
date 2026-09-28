// Compile the new modules together until the route owner wires them into services/mod.rs.
pub mod models {
    pub use crystal_forge::models::poam;
}
pub mod queries {
    pub use crystal_forge::queries::acceptance_register;
}
pub mod services {
    pub use crystal_forge::services::poam;
}
#[path = "../src/services/oscal_poam_export.rs"]
mod oscal_poam_export;
#[path = "../src/services/register_export_selection.rs"]
mod register_export_selection;
#[path = "../src/services/register_oscal_snapshot.rs"]
mod register_oscal_snapshot;

use chrono::{TimeZone, Utc};
use crystal_forge::models::poam::{PoamAssigneeView, PoamRegisterSummary, PoamSummary};
use crystal_forge::queries::acceptance_register::{AcceptanceEntry, AcceptanceSource};
use register_export_selection::{
    AcceptanceExportContext, PoamEvidenceLink, PoamExportContext, RegisterExportSelection,
    ScheduledCveTuple,
};
use register_oscal_snapshot::write_authorized;
use uuid::Uuid;

fn validate(output: &oscal_poam_export::Encodings) {
    let dir = tempfile::tempdir().unwrap();
    let json_path = dir.path().join("export.json");
    let xml_path = dir.path().join("export.xml");
    let schemas =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../schemas/oscal-1.1.2");
    std::fs::write(&json_path, &output.json).unwrap();
    std::fs::write(&xml_path, &output.xml).unwrap();
    let json = std::process::Command::new("check-jsonschema")
        .arg("--schemafile")
        .arg(schemas.join("oscal_poam_schema.json"))
        .arg(json_path)
        .output()
        .unwrap();
    assert!(
        json.status.success(),
        "{}",
        String::from_utf8_lossy(&json.stderr)
    );
    let xml = std::process::Command::new("xmllint")
        .arg("--noout")
        .arg("--schema")
        .arg(schemas.join("oscal_poam_schema.xsd"))
        .arg(xml_path)
        .output()
        .unwrap();
    assert!(
        xml.status.success(),
        "{}",
        String::from_utf8_lossy(&xml.stderr)
    );
}

fn time() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 27, 12, 0, 0)
        .single()
        .unwrap()
}

fn selection() -> RegisterExportSelection {
    let now = time();
    RegisterExportSelection {
        actor_id: Uuid::new_v4(),
        is_admin: true,
        environment_ids: vec![],
        poams: vec![PoamRegisterSummary {
            summary: PoamSummary {
                id: Uuid::new_v4(),
                human_id: "POAM-12".into(),
                title: "Replace vulnerable service".into(),
                plan: "Replace the affected service and verify it.".into(),
                owner: String::new(),
                assignee: PoamAssigneeView::Unassigned,
                target_date: None,
                risk: "high".into(),
                status: "open".into(),
                revision: 1,
                overdue: false,
                finding_count: 0,
                cve_finding_count: 1,
                created_at: now,
                updated_at: now,
                closed_at: None,
                closure_attempt_id: None,
            },
            environment_ids: vec![],
            system_ids: vec![Uuid::new_v4()],
            bundle_ids: vec![],
            bundle_version_ids: vec![],
            assignment_version_ids: vec![],
            first_requirement: None,
            first_cve: Some("CVE-2099-12345".into()),
            milestone_count: 0,
            completed_milestone_count: 0,
            last_activity_at: None,
        }],
        poam_context: vec![],
        acceptances: vec![],
        acceptance_context: vec![],
    }
}

#[test]
fn exact_single_scope_plan_uses_sourced_metadata_in_both_encodings() {
    let mut selection = selection();
    selection.poam_context.push(PoamExportContext {
        poam_id: selection.poams[0].summary.id,
        system_ids: selection.poams[0].system_ids.clone(),
        system_names: vec!["host".into()],
        visibility_system_ids: selection.poams[0].system_ids.clone(),
        environment_ids: vec![],
        environment_names: vec![],
        cve_ids: vec!["CVE-2099-12345".into(), "CVE-2099-12346".into()],
        scheduled_cve_tuples: sqlx::types::Json(vec![]),
        links: sqlx::types::Json(vec![PoamEvidenceLink {
            finding_id: Uuid::from_u128(42),
            scan_id: None,
            canonical_cve_id: None,
            canonical_package_name: None,
            description: "Policy finding".into(),
        }]),
    });
    selection.poams[0].summary.target_date = Some(time().date_naive());
    selection.poams[0].summary.owner = "Security Operations".into();
    selection.poams[0].summary.assignee = PoamAssigneeView::User {
        user_id: Uuid::from_u128(43),
        display: "Owner".into(),
        available: true,
    };
    let output = write_authorized(&selection).unwrap();
    validate(&output);
    let json: serde_json::Value = serde_json::from_str(&output.json).unwrap();
    let poam = &json["plan-of-action-and-milestones"];
    assert_eq!(poam["metadata"]["version"], "1");
    assert_eq!(
        poam["poam-items"][0]["uuid"],
        selection.poams[0].summary.id.to_string()
    );
    assert_eq!(
        poam["poam-items"][0]["description"],
        selection.poams[0].summary.plan
    );
    assert!(poam["poam-items"][0].get("related-risks").is_none());
    assert!(
        output
            .xml
            .contains(&selection.poams[0].system_ids[0].to_string())
    );
    assert!(output.xml.contains("source-target-date"));
    assert!(output.xml.contains("CVE-2099-12346"));
    let props = poam["poam-items"][0]["props"].as_array().unwrap();
    for (name, value) in [
        ("policy-finding-id", Uuid::from_u128(42).to_string()),
        ("plan-owner", "Security Operations".into()),
        ("plan-assignee-user-id", Uuid::from_u128(43).to_string()),
        ("plan-created-at", time().to_rfc3339()),
        ("plan-updated-at", time().to_rfc3339()),
    ] {
        assert!(
            props
                .iter()
                .any(|p| p["name"] == name && p["value"] == value)
        );
    }
    assert_ne!(poam["metadata"]["last-modified"], time().to_rfc3339());
    selection.poams[0].summary.owner.clear();
    selection.poams[0].summary.assignee = PoamAssigneeView::Unassigned;
    selection.poam_context[0].links.0.clear();
    let missing = write_authorized(&selection).unwrap();
    validate(&missing);
    let missing: serde_json::Value = serde_json::from_str(&missing.json).unwrap();
    let props = missing["plan-of-action-and-milestones"]["poam-items"][0]["props"]
        .as_array()
        .unwrap();
    for name in [
        "policy-finding-id",
        "plan-owner",
        "plan-assignee-user-id",
        "plan-assignee-group-name",
    ] {
        assert!(!props.iter().any(|p| p["name"] == name));
    }
    selection.poams[0].summary.assignee = PoamAssigneeView::OidcGroup {
        group_name: "security-team".into(),
        display: "Security team".into(),
        available: true,
    };
    let group = write_authorized(&selection).unwrap();
    validate(&group);
    let group: serde_json::Value = serde_json::from_str(&group.json).unwrap();
    let props = group["plan-of-action-and-milestones"]["poam-items"][0]["props"]
        .as_array()
        .unwrap();
    assert!(
        props
            .iter()
            .any(|p| p["name"] == "plan-assignee-group-name" && p["value"] == "security-team")
    );
    assert!(!props.iter().any(|p| p["name"] == "plan-assignee-user-id"));
}

#[test]
fn incomplete_context_refuses_the_entire_selection() {
    let mut rows = selection();
    assert!(
        write_authorized(&rows)
            .unwrap_err()
            .to_string()
            .contains("incomplete source context")
    );
    rows.poam_context.push(PoamExportContext {
        poam_id: rows.poams[0].summary.id,
        system_ids: vec![Uuid::new_v4()],
        system_names: vec!["other".into()],
        visibility_system_ids: vec![],
        environment_ids: vec![],
        environment_names: vec![],
        cve_ids: vec![],
        scheduled_cve_tuples: sqlx::types::Json(vec![]),
        links: sqlx::types::Json(vec![]),
    });
    rows.is_admin = false;
    assert!(
        write_authorized(&rows)
            .unwrap_err()
            .to_string()
            .contains("hidden linked context")
    );
    rows.is_admin = true;
    rows.poams[0].environment_ids.push(Uuid::new_v4());
    assert!(write_authorized(&rows).is_ok());
}

#[test]
fn exact_cve_links_keep_package_finding_and_optional_baseline_in_both_encodings() {
    let mut rows = selection();
    let finding = Uuid::from_u128(50);
    let scan = Uuid::from_u128(51);
    rows.poam_context.push(PoamExportContext {
        poam_id: rows.poams[0].summary.id,
        system_ids: rows.poams[0].system_ids.clone(),
        system_names: vec!["host".into()],
        visibility_system_ids: rows.poams[0].system_ids.clone(),
        environment_ids: vec![],
        environment_names: vec![],
        cve_ids: vec!["CVE-2099-12345".into()],
        scheduled_cve_tuples: sqlx::types::Json(vec![]),
        links: sqlx::types::Json(vec![
            PoamEvidenceLink {
                finding_id: finding,
                scan_id: Some(scan),
                canonical_cve_id: Some("CVE-2099-12345".into()),
                canonical_package_name: Some("openssl".into()),
                description: "CVE-2099-12345 / openssl".into(),
            },
            PoamEvidenceLink {
                finding_id: Uuid::from_u128(52),
                scan_id: None,
                canonical_cve_id: Some("CVE-2099-12345".into()),
                canonical_package_name: Some("libssl".into()),
                description: "CVE-2099-12345 / libssl".into(),
            },
        ]),
    });
    let output = write_authorized(&rows).unwrap();
    validate(&output);
    let json: serde_json::Value = serde_json::from_str(&output.json).unwrap();
    let props = json["plan-of-action-and-milestones"]["poam-items"][0]["props"]
        .as_array()
        .unwrap();
    let links: Vec<serde_json::Value> = props
        .iter()
        .filter(|p| p["name"] == "cve-finding-link")
        .map(|p| {
            assert_eq!(p["ns"], "https://crystalforge.dev/ns/oscal/poam/1.0");
            serde_json::from_str(p["value"].as_str().unwrap()).unwrap()
        })
        .collect();
    assert_eq!(links.len(), 2);
    assert!(links.iter().any(|l| l["finding_id"] == finding.to_string()
        && l["canonical_package_name"] == "openssl"
        && l["baseline_scan_id"] == scan.to_string()));
    assert!(links.iter().any(|l| l["canonical_package_name"] == "libssl"
        && l.get("baseline_scan_id").is_none()));
    assert!(!props.iter().any(|p| p["name"] == "policy-finding-id"));
    assert!(output.xml.contains("cve-finding-link"));
    assert!(
        json["plan-of-action-and-milestones"]
            .get("observations")
            .is_none()
    );
}

#[test]
fn scheduled_environment_without_links_keeps_exact_tuples_without_observations() {
    let mut rows = selection();
    rows.poams[0].system_ids.clear();
    let environment = Uuid::new_v4();
    rows.poams[0].environment_ids.push(environment);
    rows.poam_context.push(PoamExportContext {
        poam_id: rows.poams[0].summary.id,
        system_ids: vec![],
        system_names: vec![],
        visibility_system_ids: vec![],
        environment_ids: vec![environment],
        environment_names: vec!["source environment".into()],
        cve_ids: vec!["CVE-2099-12345".into()],
        scheduled_cve_tuples: sqlx::types::Json(vec![
            ScheduledCveTuple {
                canonical_cve_id: "CVE-2099-12345".into(),
                canonical_package_name: "libssl".into(),
            },
            ScheduledCveTuple {
                canonical_cve_id: "CVE-2099-12345".into(),
                canonical_package_name: "openssl".into(),
            },
        ]),
        links: sqlx::types::Json(vec![]),
    });
    let output = write_authorized(&rows).unwrap();
    validate(&output);
    let json: serde_json::Value = serde_json::from_str(&output.json).unwrap();
    let item = &json["plan-of-action-and-milestones"]["poam-items"][0];
    let props = item["props"].as_array().unwrap();
    let packages: Vec<_> = props.iter().filter(|p| p["name"] == "scheduled-environment-cve-tuple")
        .map(|p| serde_json::from_str::<serde_json::Value>(p["value"].as_str().unwrap()).unwrap()["canonical_package_name"].as_str().unwrap().to_owned()).collect();
    assert_eq!(packages, ["libssl", "openssl"]);
    assert!(!props.iter().any(|p| p["name"] == "cve-finding-link"));
    assert!(
        json["plan-of-action-and-milestones"]
            .get("findings")
            .is_none()
    );
    assert!(
        json["plan-of-action-and-milestones"]
            .get("observations")
            .is_none()
    );
    rows.is_admin = false;
    rows.poams[0].environment_ids.clear();
    assert!(
        write_authorized(&rows)
            .unwrap_err()
            .to_string()
            .contains("hidden linked context")
    );
}

#[test]
fn accepted_decision_with_justification_stays_a_decision() {
    let mut rows = selection();
    rows.acceptances.push(AcceptanceEntry {
        source: AcceptanceSource::CveHost,
        source_id: Uuid::new_v4(),
        waiver_updated_at: None,
        status: "accepted".into(),
        finding_id: None,
        system_id: Some(rows.poams[0].system_ids[0]),
        environment_id: None,
        policy_lineage_id: None,
        policy_version_id: None,
        canonical_cve_id: Some("CVE-2099-12345".into()),
        canonical_package_name: Some("openssl".into()),
        justification: "Risk accepted".into(),
        review_date: Some(time().date_naive()),
        review_due_at: Some(time().date_naive()),
        expires_at: Some(time()),
        accepted_by: Some(rows.actor_id),
        accepted_at: Some(time()),
        retired_at: Some(time()),
        retired_by: Some(rows.actor_id),
        retirement_reason: Some("Converted to remediation".into()),
        replacement_poam_id: Some(rows.poams[0].summary.id),
        recorded_at: time(),
    });
    rows.poam_context.push(PoamExportContext {
        poam_id: rows.poams[0].summary.id,
        system_ids: rows.poams[0].system_ids.clone(),
        system_names: vec!["host".into()],
        visibility_system_ids: rows.poams[0].system_ids.clone(),
        environment_ids: vec![],
        environment_names: vec![],
        cve_ids: vec!["CVE-2099-12345".into()],
        scheduled_cve_tuples: sqlx::types::Json(vec![]),
        links: sqlx::types::Json(vec![]),
    });
    rows.acceptance_context.push(AcceptanceExportContext {
        source_id: rows.acceptances[0].source_id,
        scope_name: "host".into(),
        policy_name: None,
        finding_id: None,
    });
    let output = write_authorized(&rows).unwrap();
    validate(&output);
    let json: serde_json::Value = serde_json::from_str(&output.json).unwrap();
    let poam = &json["plan-of-action-and-milestones"];
    assert_eq!(poam["poam-items"].as_array().unwrap().len(), 2);
    assert!(poam.get("risks").is_none());
    let props = poam["poam-items"][1]["props"].as_array().unwrap();
    assert!(props.iter().any(|p| p["name"] == "decision-rationale"));
    assert!(props.iter().any(|p| p["name"] == "accepted-by-user-id"));
    for (name, value) in [
        ("decision-recorded-at", time().to_rfc3339()),
        ("decision-review-date", "2026-09-27".into()),
        ("decision-review-due-date", "2026-09-27".into()),
        ("decision-expires-at", time().to_rfc3339()),
        ("decision-retired-at", time().to_rfc3339()),
        ("decision-retired-by-user-id", rows.actor_id.to_string()),
        (
            "decision-retirement-reason",
            "Converted to remediation".into(),
        ),
        (
            "decision-replacement-poam-id",
            rows.poams[0].summary.id.to_string(),
        ),
    ] {
        assert!(
            props
                .iter()
                .any(|p| p["name"] == name && p["value"] == value)
        );
    }
    assert!(!props.iter().any(|p| p["name"] == "source-target-date"));
    rows.acceptances[0].expires_at = None;
    rows.acceptances[0].retired_at = None;
    rows.acceptances[0].retired_by = None;
    rows.acceptances[0].retirement_reason = None;
    rows.acceptances[0].replacement_poam_id = None;
    rows.acceptances[0].review_date = None;
    rows.acceptances[0].review_due_at = None;
    let absent = write_authorized(&rows).unwrap();
    validate(&absent);
    let absent: serde_json::Value = serde_json::from_str(&absent.json).unwrap();
    let props = absent["plan-of-action-and-milestones"]["poam-items"][1]["props"]
        .as_array()
        .unwrap();
    for name in [
        "decision-expires-at",
        "decision-retired-at",
        "decision-retired-by-user-id",
        "decision-retirement-reason",
        "decision-replacement-poam-id",
        "decision-review-date",
        "decision-review-due-date",
    ] {
        assert!(!props.iter().any(|p| p["name"] == name));
    }
}
