//! XCCDF 1.2 XML writer for CF-XCCDF bundle export.
//!
//! Produces a standards-valid XCCDF 1.2 Benchmark with Crystal Forge extension
//! elements. All XML text is escaped automatically by quick-xml; no CDATA
//! sections are constructed via string concatenation.

use std::collections::BTreeMap;
use std::io::Cursor;

use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Writer;

use super::super::canonical::{ImplementationState, PublicationState};
use super::super::interchange::{
    CF_NIX_FIX_SYSTEM, CF_POLICY_CHECK_SYSTEM, CF_XCCDF_NAMESPACE, XCCDF_1_2_NAMESPACE,
};
use super::export_models::{XccdfBundleExport, XccdfPolicyExport, XccdfSourceMapping};

fn publication_state_str(s: PublicationState) -> &'static str {
    match s {
        PublicationState::Incomplete => "incomplete",
        PublicationState::Draft | PublicationState::Interim | PublicationState::Accepted => "draft",
        PublicationState::Deprecated => "deprecated",
    }
}

fn implementation_state_str(s: ImplementationState) -> &'static str {
    match s {
        ImplementationState::Native => "native",
        ImplementationState::Manual => "manual",
        ImplementationState::External => "external",
        ImplementationState::Unbound => "unbound",
        ImplementationState::Opaque => "opaque",
    }
}

fn severity_for_type(policy_type: &str) -> &'static str {
    match policy_type {
        "require_cf_agent" | "require_packages" | "custom_check" | "time_window"
        | "canary_rollout" => "medium",
        "require_cve_check" | "require_approvals" | "cve_threshold" => "high",
        _ => "medium",
    }
}

fn group_title_for_type(policy_type: &str) -> &'static str {
    match policy_type {
        "require_cf_agent" => "Crystal Forge Agent Requirement",
        "require_packages" => "Required Package Verification",
        "custom_check" => "Custom Policy Check",
        "require_cve_check" => "CVE Check Gate",
        "cve_threshold" => "CVE Threshold Gate",
        "time_window" => "Time Window Policy",
        "require_approvals" => "Approval Gate",
        "canary_rollout" => "Canary Rollout",
        _ => "Policy Group",
    }
}

fn group_id_for_type(policy_type: &str) -> String {
    let slug = policy_type.replace('_', "-");
    format!("xccdf_org.crystalforge.group:{slug}")
}

fn is_nix_policy(policy_type: &str) -> bool {
    matches!(
        policy_type,
        "require_cf_agent" | "require_packages" | "custom_check"
    )
}

fn el(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    name: &str,
    text: &str,
) -> Result<(), std::io::Error> {
    writer.write_event(Event::Start(BytesStart::new(name)))?;
    writer.write_event(Event::Text(BytesText::new(text)))?;
    writer.write_event(Event::End(BytesEnd::new(name)))
}

fn cf_el(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    tag: &str,
    text: &str,
) -> Result<(), std::io::Error> {
    let full = format!("cf:{tag}");
    el(writer, &full, text)
}

fn cf_empty(writer: &mut Writer<Cursor<&mut Vec<u8>>>, tag: &str) -> Result<(), std::io::Error> {
    let full = format!("cf:{tag}");
    writer.write_event(Event::Empty(BytesStart::new(&full)))
}

fn write_source_mappings(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    mappings: &[XccdfSourceMapping],
) -> Result<(), std::io::Error> {
    if mappings.is_empty() {
        return Ok(());
    }
    writer.write_event(Event::Start(BytesStart::new("cf:source-mappings")))?;
    for m in mappings {
        writer.write_event(Event::Start(BytesStart::new("cf:source")))?;
        el(writer, "cf:object-kind", &m.object_kind)?;
        el(writer, "cf:source-identity", &m.source_identity)?;
        el(writer, "cf:fidelity", &m.fidelity)?;
        writer.write_event(Event::End(BytesEnd::new("cf:source")))?;
    }
    writer.write_event(Event::End(BytesEnd::new("cf:source-mappings")))?;
    Ok(())
}

fn write_policy_identity(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    pv: &XccdfPolicyExport,
) -> Result<(), std::io::Error> {
    let policy_urn = format!("urn:uuid:{}", pv.policy_id);
    let version_urn = format!("urn:uuid:{}", pv.policy_version_id);

    let mut elem = BytesStart::new("cf:policy-identity");
    elem.push_attribute(("policy-id", policy_urn.as_str()));
    elem.push_attribute(("policy-version-id", version_urn.as_str()));
    elem.push_attribute(("publication-state", publication_state_str(pv.publication_state)));
    writer.write_event(Event::Start(elem))?;

    cf_el(writer, "policy-version", &pv.version)?;

    let mut digest = BytesStart::new("cf:content-digest");
    digest.push_attribute(("algorithm", "sha-256"));
    writer.write_event(Event::Start(digest))?;
    writer.write_event(Event::Text(BytesText::new(&pv.semantic_digest)))?;
    writer.write_event(Event::End(BytesEnd::new("cf:content-digest")))?;

    writer.write_event(Event::End(BytesEnd::new("cf:policy-identity")))?;
    Ok(())
}

fn write_cf_policy(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    pv: &XccdfPolicyExport,
) -> Result<(), std::io::Error> {
    let mut policy = BytesStart::new("cf:policy");
    policy.push_attribute(("schema-version", "1"));
    writer.write_event(Event::Start(policy))?;

    let mut exec = BytesStart::new("cf:execution");
    exec.push_attribute(("phase", pv.execution_phase.as_str()));
    exec.push_attribute(("strict", "true"));
    writer.write_event(Event::Empty(exec))?;

    write_implementation(writer, pv)?;

    if !pv.dependencies.is_null() {
        let deps_val = match &pv.dependencies {
            serde_json::Value::Array(a) if a.is_empty() => None,
            serde_json::Value::Object(o) if o.is_empty() => None,
            other => Some(other),
        };
        if let Some(deps) = deps_val {
            writer.write_event(Event::Start(BytesStart::new("cf:dependencies")))?;
            if let serde_json::Value::Array(items) = deps {
                for item in items {
                    if let serde_json::Value::String(s) = item {
                        cf_el(writer, "nix-option", s)?;
                    }
                }
            }
            writer.write_event(Event::End(BytesEnd::new("cf:dependencies")))?;
        }
    }

    writer.write_event(Event::End(BytesEnd::new("cf:policy")))?;
    Ok(())
}

fn write_implementation(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    pv: &XccdfPolicyExport,
) -> Result<(), std::io::Error> {
    writer.write_event(Event::Start(BytesStart::new("cf:implementation")))?;

    match pv.policy_type.as_str() {
        "require_cf_agent" => {
            cf_empty(writer, "require-crystal-forge-agent")?;
        }
        "require_packages" => {
            writer.write_event(Event::Start(BytesStart::new("cf:require-packages")))?;
            if let Some(pkgs) = pv.config.get("packages").and_then(|v| v.as_array()) {
                for pkg in pkgs {
                    if let serde_json::Value::String(name) = pkg {
                        cf_el(writer, "package", name)?;
                    }
                }
            }
            writer.write_event(Event::End(BytesEnd::new("cf:require-packages")))?;
        }
        "custom_check" => {
            write_custom_check(writer, pv)?;
        }
        "require_cve_check" => {
            let mut elem = BytesStart::new("cf:require-cve-check");
            let max_crit = pv.config.get("max_critical").and_then(|v| v.as_u64()).unwrap_or(0);
            let req_just = pv
                .config
                .get("require_high_justification")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let no_scan = pv
                .config
                .get("when_no_scan")
                .and_then(|v| v.as_str())
                .unwrap_or("block");
            elem.push_attribute(("max-critical", &*max_crit.to_string()));
            elem.push_attribute(("require-high-justification", &*req_just.to_string()));
            elem.push_attribute(("when-no-scan", no_scan));
            writer.write_event(Event::Start(elem))?;
            if let Some(max_high) = pv.config.get("max_high").and_then(|v| v.as_u64()) {
                cf_el(writer, "max-high", &max_high.to_string())?;
            }
            writer.write_event(Event::End(BytesEnd::new("cf:require-cve-check")))?;
        }
        "time_window" => {
            let mut elem = BytesStart::new("cf:time-window");
            let start = pv
                .config
                .get("start_time")
                .and_then(|v| v.as_str())
                .unwrap_or("00:00");
            let end = pv
                .config
                .get("end_time")
                .and_then(|v| v.as_str())
                .unwrap_or("23:59");
            let tz = pv
                .config
                .get("timezone")
                .and_then(|v| v.as_str())
                .unwrap_or("UTC");
            let action = pv
                .config
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("block");
            elem.push_attribute(("start-time", start));
            elem.push_attribute(("end-time", end));
            elem.push_attribute(("timezone", tz));
            elem.push_attribute(("action", action));
            writer.write_event(Event::Start(elem))?;
            if let Some(desc) = pv.config.get("description").and_then(|v| v.as_str()) {
                cf_el(writer, "description", desc)?;
            }
            if let Some(days) = pv.config.get("days").and_then(|v| v.as_array()) {
                for day in days {
                    if let serde_json::Value::String(d) = day {
                        cf_el(writer, "day", d)?;
                    }
                }
            }
            writer.write_event(Event::End(BytesEnd::new("cf:time-window")))?;
        }
        "require_approvals" => {
            let mut elem = BytesStart::new("cf:require-approvals");
            let count = pv
                .config
                .get("count")
                .and_then(|v| v.as_u64())
                .unwrap_or(1);
            let role = pv
                .config
                .get("role")
                .and_then(|v| v.as_str())
                .unwrap_or("admin");
            let distinct = pv
                .config
                .get("distinct")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            elem.push_attribute(("count", &*count.to_string()));
            elem.push_attribute(("role", role));
            elem.push_attribute(("distinct", &*distinct.to_string()));
            writer.write_event(Event::Start(elem))?;
            if let Some(desc) = pv.config.get("description").and_then(|v| v.as_str()) {
                cf_el(writer, "description", desc)?;
            }
            if let Some(hours) = pv
                .config
                .get("expires_after_hours")
                .and_then(|v| v.as_u64())
            {
                cf_el(writer, "expires-after-hours", &hours.to_string())?;
            }
            writer.write_event(Event::End(BytesEnd::new("cf:require-approvals")))?;
        }
        "canary_rollout" => {
            let mut elem = BytesStart::new("cf:canary-rollout");
            let pct = pv
                .config
                .get("percentage")
                .and_then(|v| v.as_u64())
                .unwrap_or(10);
            let dur = pv
                .config
                .get("observe_duration_minutes")
                .and_then(|v| v.as_u64())
                .unwrap_or(30);
            let strat = pv
                .config
                .get("selection_strategy")
                .and_then(|v| v.as_str())
                .unwrap_or("random");
            elem.push_attribute(("percentage", &*pct.to_string()));
            elem.push_attribute(("observe-duration-minutes", &*dur.to_string()));
            elem.push_attribute(("selection-strategy", strat));
            writer.write_event(Event::Start(elem))?;
            if let Some(desc) = pv.config.get("description").and_then(|v| v.as_str()) {
                cf_el(writer, "description", desc)?;
            }
            if let Some(hc) = pv.config.get("health_check") {
                let mut hc_elem = BytesStart::new("cf:health-check");
                let hc_type = hc.get("type").and_then(|v| v.as_str()).unwrap_or("http");
                let fail = hc
                    .get("fail_threshold")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(3);
                hc_elem.push_attribute(("type", hc_type));
                hc_elem.push_attribute(("fail-threshold", &*fail.to_string()));
                writer.write_event(Event::Empty(hc_elem))?;
            }
            writer.write_event(Event::End(BytesEnd::new("cf:canary-rollout")))?;
        }
        "cve_threshold" => {
            let mut elem = BytesStart::new("cf:cve-threshold");
            let no_scan = pv
                .config
                .get("no_scan_behavior")
                .and_then(|v| v.as_str())
                .unwrap_or("block");
            let allow_just = pv
                .config
                .get("allow_justifications")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let req_ack = pv
                .config
                .get("require_acknowledgment")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            elem.push_attribute(("no-scan-behavior", no_scan));
            elem.push_attribute(("allow-justifications", &*allow_just.to_string()));
            elem.push_attribute(("require-acknowledgment", &*req_ack.to_string()));
            writer.write_event(Event::Start(elem))?;
            if let Some(desc) = pv.config.get("description").and_then(|v| v.as_str()) {
                cf_el(writer, "description", desc)?;
            }
            if let Some(thresholds) = pv.config.get("thresholds").and_then(|v| v.as_array()) {
                for t in thresholds {
                    let mut t_elem = BytesStart::new("cf:threshold");
                    let sev = t.get("severity").and_then(|v| v.as_str()).unwrap_or("high");
                    let max = t.get("max").and_then(|v| v.as_u64()).unwrap_or(0);
                    let act = t.get("action").and_then(|v| v.as_str()).unwrap_or("block");
                    t_elem.push_attribute(("severity", sev));
                    t_elem.push_attribute(("max", &*max.to_string()));
                    t_elem.push_attribute(("action", act));
                    writer.write_event(Event::Empty(t_elem))?;
                }
            }
            writer.write_event(Event::End(BytesEnd::new("cf:cve-threshold")))?;
        }
        _ => {
            cf_empty(writer, "require-crystal-forge-agent")?;
        }
    }

    writer.write_event(Event::End(BytesEnd::new("cf:implementation")))?;
    Ok(())
}

fn write_custom_check(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    pv: &XccdfPolicyExport,
) -> Result<(), std::io::Error> {
    let rules = pv.config.get("rules").and_then(|v| v.as_array());
    let has_rules = rules.map(|r| !r.is_empty()).unwrap_or(false);

    let mode = pv
        .config
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("all");
    let context = pv
        .config
        .get("context")
        .and_then(|v| v.as_str())
        .unwrap_or("nix-eval");
    let binding = pv
        .config
        .get("binding")
        .and_then(|v| v.as_str())
        .unwrap_or("config");

    let mut elem = BytesStart::new("cf:custom-check");
    elem.push_attribute(("mode", mode));
    elem.push_attribute(("context", context));
    elem.push_attribute(("binding", binding));
    writer.write_event(Event::Start(elem))?;

    if has_rules {
        for rule in rules.unwrap() {
            write_custom_check_rule(writer, rule)?;
        }
    } else {
        let mut rule_elem = BytesStart::new("cf:rule");
        let field_name = pv
            .config
            .get("field_name")
            .and_then(|v| v.as_str())
            .unwrap_or("enabled");
        let strict = pv
            .config
            .get("strict")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        rule_elem.push_attribute(("field-name", field_name));
        rule_elem.push_attribute(("strict", &*strict.to_string()));
        writer.write_event(Event::Start(rule_elem))?;
        if let Some(desc) = pv.config.get("description").and_then(|v| v.as_str()) {
            cf_el(writer, "description", desc)?;
        }
        if let Some(expr) = pv.config.get("expression").and_then(|v| v.as_str()) {
            let mut expr_elem = BytesStart::new("cf:expression");
            expr_elem.push_attribute(("language", "nix"));
            writer.write_event(Event::Start(expr_elem))?;
            writer.write_event(Event::Text(BytesText::new(expr)))?;
            writer.write_event(Event::End(BytesEnd::new("cf:expression")))?;
        }
        writer.write_event(Event::End(BytesEnd::new("cf:rule")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("cf:custom-check")))?;
    Ok(())
}

fn write_custom_check_rule(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    rule: &serde_json::Value,
) -> Result<(), std::io::Error> {
    let mut elem = BytesStart::new("cf:rule");
    let field_name = rule
        .get("field_name")
        .and_then(|v| v.as_str())
        .unwrap_or("enabled");
    let strict = rule
        .get("strict")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    elem.push_attribute(("field-name", field_name));
    elem.push_attribute(("strict", &*strict.to_string()));
    writer.write_event(Event::Start(elem))?;
    if let Some(desc) = rule.get("description").and_then(|v| v.as_str()) {
        cf_el(writer, "description", desc)?;
    }
    if let Some(expr) = rule.get("expression").and_then(|v| v.as_str()) {
        let mut expr_elem = BytesStart::new("cf:expression");
        expr_elem.push_attribute(("language", "nix"));
        writer.write_event(Event::Start(expr_elem))?;
        writer.write_event(Event::Text(BytesText::new(expr)))?;
        writer.write_event(Event::End(BytesEnd::new("cf:expression")))?;
    }
    writer.write_event(Event::End(BytesEnd::new("cf:rule")))?;
    Ok(())
}

fn write_check_and_fix(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    pv: &XccdfPolicyExport,
) -> Result<(), std::io::Error> {
    match pv.implementation_state {
        ImplementationState::Native => {
            let mut check = BytesStart::new("check");
            check.push_attribute(("system", CF_POLICY_CHECK_SYSTEM));
            writer.write_event(Event::Start(check))?;
            let mut ccr = BytesStart::new("check-content-ref");
            ccr.push_attribute(("href", "crystal-forge://policy"));
            ccr.push_attribute(("name", pv.policy_type.as_str()));
            writer.write_event(Event::Empty(ccr))?;
            writer.write_event(Event::End(BytesEnd::new("check")))?;

            let fix_id = format!("{}.fix", pv.policy_id);
            let mut fix = BytesStart::new("fix");
            fix.push_attribute(("system", CF_NIX_FIX_SYSTEM));
            fix.push_attribute(("id", fix_id.as_str()));
            writer.write_event(Event::Start(fix))?;
            let fix_text = format!(
                "Apply {} policy via Nix evaluation",
                pv.policy_type
            );
            writer.write_event(Event::Text(BytesText::new(&fix_text)))?;
            writer.write_event(Event::End(BytesEnd::new("fix")))?;
        }
        ImplementationState::Manual => {
            let mut check = BytesStart::new("check");
            check.push_attribute(("system", CF_POLICY_CHECK_SYSTEM));
            writer.write_event(Event::Start(check))?;
            let text = format!(
                "Manual policy ({}) – user must provide evidence of compliance",
                pv.policy_type
            );
            writer.write_event(Event::Text(BytesText::new(&text)))?;
            writer.write_event(Event::End(BytesEnd::new("check")))?;
        }
        ImplementationState::Unbound => {
            let mut check = BytesStart::new("check");
            check.push_attribute(("system", CF_POLICY_CHECK_SYSTEM));
            writer.write_event(Event::Start(check))?;
            let text = format!(
                "Unbound policy ({}) – requirement exists but has no implementation",
                pv.policy_type
            );
            writer.write_event(Event::Text(BytesText::new(&text)))?;
            writer.write_event(Event::End(BytesEnd::new("check")))?;
        }
        ImplementationState::External => {
            let mut check = BytesStart::new("check");
            check.push_attribute(("system", CF_POLICY_CHECK_SYSTEM));
            writer.write_event(Event::Start(check))?;
            let text = format!(
                "External policy ({}) – checked by external system",
                pv.policy_type
            );
            writer.write_event(Event::Text(BytesText::new(&text)))?;
            writer.write_event(Event::End(BytesEnd::new("check")))?;
        }
        ImplementationState::Opaque => {
            let mut check = BytesStart::new("check");
            check.push_attribute(("system", CF_POLICY_CHECK_SYSTEM));
            writer.write_event(Event::Start(check))?;
            let text = format!(
                "Opaque policy ({}) – CF preserves rule but cannot model check",
                pv.policy_type
            );
            writer.write_event(Event::Text(BytesText::new(&text)))?;
            writer.write_event(Event::End(BytesEnd::new("check")))?;
        }
    }
    Ok(())
}

fn write_standard_idents(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    pv: &XccdfPolicyExport,
) -> Result<(), std::io::Error> {
    if let Some(idents) = pv
        .compliance_metadata
        .get("identifiers")
        .and_then(|v| v.as_array())
    {
        for ident in idents {
            let system = ident.get("system").and_then(|v| v.as_str()).unwrap_or("");
            let value = ident.get("value").and_then(|v| v.as_str()).unwrap_or("");
            if !system.is_empty() && !value.is_empty() {
                let mut elem = BytesStart::new("ident");
                elem.push_attribute(("system", system));
                writer.write_event(Event::Start(elem))?;
                writer.write_event(Event::Text(BytesText::new(value)))?;
                writer.write_event(Event::End(BytesEnd::new("ident")))?;
            }
        }
    }
    Ok(())
}

fn write_standard_references(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    pv: &XccdfPolicyExport,
) -> Result<(), std::io::Error> {
    if let Some(refs) = pv
        .compliance_metadata
        .get("references")
        .and_then(|v| v.as_array())
    {
        for r in refs {
            let href = r.get("href").and_then(|v| v.as_str()).unwrap_or("");
            let title = r.get("title").and_then(|v| v.as_str()).unwrap_or("");
            if !href.is_empty() {
                let mut elem = BytesStart::new("reference");
                elem.push_attribute(("href", href));
                writer.write_event(Event::Start(elem))?;
                if !title.is_empty() {
                    writer.write_event(Event::Text(BytesText::new(title)))?;
                }
                writer.write_event(Event::End(BytesEnd::new("reference")))?;
            }
        }
    }
    Ok(())
}

fn write_rule_content(
    writer: &mut Writer<Cursor<&mut Vec<u8>>>,
    pv: &XccdfPolicyExport,
) -> Result<(), std::io::Error> {
    el(writer, "title", &pv.name)?;
    if let Some(ref desc) = pv.description {
        el(writer, "description", desc)?;
    }
    el(writer, "version", &pv.version)?;

    write_policy_identity(writer, pv)?;
    write_cf_policy(writer, pv)?;

    write_standard_idents(writer, pv)?;
    write_standard_references(writer, pv)?;

    write_check_and_fix(writer, pv)?;

    if let Some(ref opaque) = pv.opaque_xml {
        cf_el(writer, "opaque-xml", opaque)?;
    }

    if !pv.source_mappings.is_empty() {
        write_source_mappings(writer, &pv.source_mappings)?;
    }

    Ok(())
}

/// Write a complete XCCDF 1.2 Benchmark for a bundle version export.
pub fn write_bundle_xccdf_export(
    snapshot: &XccdfBundleExport,
) -> Result<String, std::io::Error> {
    let mut buf = Vec::new();
    let mut writer = Writer::new(Cursor::new(&mut buf));

    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;

    let benchmark_id = snapshot.benchmark_id();
    let mut bench = BytesStart::new("Benchmark");
    bench.push_attribute(("xmlns", XCCDF_1_2_NAMESPACE));
    bench.push_attribute(("xmlns:cf", CF_XCCDF_NAMESPACE));
    bench.push_attribute(("id", benchmark_id.as_str()));
    writer.write_event(Event::Start(bench))?;

    el(
        &mut writer,
        "status",
        publication_state_str(snapshot.publication_state),
    )?;
    el(&mut writer, "title", &snapshot.name)?;
    if let Some(ref desc) = snapshot.description {
        el(&mut writer, "description", desc)?;
    }
    el(
        &mut writer,
        "version",
        snapshot.framework_version.as_deref().unwrap_or("0.1.0"),
    )?;

    writer.write_event(Event::Start(BytesStart::new("metadata")))?;

    writer.write_event(Event::Start(BytesStart::new("cf:bundle")))?;

    let mut fw_elem = BytesStart::new("cf:framework");
    fw_elem.push_attribute(("name", snapshot.framework.as_str()));
    let fw_ver = snapshot
        .framework_version
        .as_deref()
        .unwrap_or("0.1.0");
    fw_elem.push_attribute(("version", fw_ver));
    writer.write_event(Event::Empty(fw_elem))?;

    cf_el(&mut writer, "layer", &snapshot.layer)?;
    cf_el(&mut writer, "owner", &snapshot.owner)?;
    cf_el(&mut writer, "semantic-digest", &snapshot.semantic_digest)?;
    cf_el(
        &mut writer,
        "publication-state",
        publication_state_str(snapshot.publication_state),
    )?;

    writer.write_event(Event::End(BytesEnd::new("cf:bundle")))?;

    let has_source_mappings = snapshot
        .policies
        .iter()
        .any(|p| !p.source_mappings.is_empty());
    if has_source_mappings {
        let all_mappings: Vec<_> = snapshot
            .policies
            .iter()
            .flat_map(|p| p.source_mappings.iter().cloned())
            .collect();
        write_source_mappings(&mut writer, &all_mappings)?;
    }

    let mut bundle_id_elem = BytesStart::new("cf:bundle-id");
    let bundle_urn = format!("urn:uuid:{}", snapshot.bundle_id);
    bundle_id_elem.push_attribute(("system", "urn:crystal-forge:bundle:1"));
    writer.write_event(Event::Start(bundle_id_elem))?;
    writer.write_event(Event::Text(BytesText::new(&bundle_urn)))?;
    writer.write_event(Event::End(BytesEnd::new("cf:bundle-id")))?;

    let mut bundle_vid_elem = BytesStart::new("cf:bundle-version-id");
    let bundle_v_urn = format!("urn:uuid:{}", snapshot.bundle_version_id);
    bundle_vid_elem.push_attribute(("system", "urn:crystal-forge:bundle-version:1"));
    writer.write_event(Event::Start(bundle_vid_elem))?;
    writer.write_event(Event::Text(BytesText::new(&bundle_v_urn)))?;
    writer.write_event(Event::End(BytesEnd::new("cf:bundle-version-id")))?;

    writer.write_event(Event::End(BytesEnd::new("metadata")))?;

    let profile_id = snapshot.profile_id();
    let mut prof = BytesStart::new("Profile");
    prof.push_attribute(("id", profile_id.as_str()));
    writer.write_event(Event::Start(prof))?;
    el(&mut writer, "title", "Crystal Forge Baseline")?;
    for policy in &snapshot.policies {
        let rid = policy.rule_id();
        let mut sel = BytesStart::new("select");
        sel.push_attribute(("idref", rid.as_str()));
        sel.push_attribute((
            "selected",
            if policy.selected {
                "true"
            } else {
                "false"
            },
        ));
        writer.write_event(Event::Empty(sel))?;
    }
    writer.write_event(Event::End(BytesEnd::new("Profile")))?;

    let mut groups: BTreeMap<String, Vec<&XccdfPolicyExport>> = BTreeMap::new();
    for policy in &snapshot.policies {
        groups
            .entry(policy.policy_type.clone())
            .or_default()
            .push(policy);
    }

    for (policy_type, policies) in &groups {
        let gid = group_id_for_type(policy_type);
        let mut group = BytesStart::new("Group");
        group.push_attribute(("id", gid.as_str()));
        writer.write_event(Event::Start(group))?;
        el(&mut writer, "title", group_title_for_type(policy_type))?;

        for pv in policies {
            let rid = pv.rule_id();
            let mut rule = BytesStart::new("Rule");
            rule.push_attribute(("id", rid.as_str()));
            rule.push_attribute((
                "selected",
                if pv.selected {
                    "true"
                } else {
                    "false"
                },
            ));
            rule.push_attribute(("weight", "10.0"));
            rule.push_attribute(("severity", severity_for_type(&pv.policy_type)));
            writer.write_event(Event::Start(rule))?;

            write_rule_content(&mut writer, pv)?;

            writer.write_event(Event::End(BytesEnd::new("Rule")))?;
        }

        writer.write_event(Event::End(BytesEnd::new("Group")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Benchmark")))?;
    drop(writer);

    Ok(String::from_utf8(buf).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::export_models::{XccdfPolicyExport, XccdfSourceMapping};
    use crate::compliance::canonical::{ImplementationState, PublicationState};
    use serde_json::json;
    use uuid::Uuid;

    fn test_policy(
        policy_type: &str,
        impl_state: ImplementationState,
        config: serde_json::Value,
    ) -> XccdfPolicyExport {
        let id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let vid = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
        XccdfPolicyExport {
            policy_id: id,
            policy_version_id: vid,
            version: "1.0".into(),
            publication_state: PublicationState::Draft,
            semantic_digest: "abc123".into(),
            name: "Test Policy".into(),
            description: Some("A test policy".into()),
            policy_type: policy_type.into(),
            execution_phase: "nix-evaluation".into(),
            implementation_state: impl_state,
            enabled_default: true,
            selected: true,
            policy_order: 1,
            config,
            compliance_metadata: json!({}),
            dependencies: json!([]),
            opaque_xml: None,
            source_mappings: vec![],
        }
    }

    fn test_snapshot() -> XccdfBundleExport {
        let bundle_id = Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap();
        let bundle_version_id = Uuid::parse_str("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb").unwrap();

        let p1_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let p1_vid = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
        let p2_id = Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap();
        let p2_vid = Uuid::parse_str("44444444-4444-4444-4444-444444444444").unwrap();

        XccdfBundleExport {
            bundle_id,
            bundle_version_id,
            version: "1.0".into(),
            publication_state: PublicationState::Draft,
            semantic_digest: "abc123".into(),
            name: "Test Bundle".into(),
            description: Some("A test bundle".into()),
            framework: "STIG".into(),
            framework_version: Some("V1R1".into()),
            layer: "os".into(),
            owner: "Team".into(),
            policies: vec![
                XccdfPolicyExport {
                    policy_id: p1_id,
                    policy_version_id: p1_vid,
                    version: "1.0".into(),
                    publication_state: PublicationState::Accepted,
                    semantic_digest: "digest1".into(),
                    name: "CF Agent Policy".into(),
                    description: Some("Requires CF agent".into()),
                    policy_type: "require_cf_agent".into(),
                    execution_phase: "nix-evaluation".into(),
                    implementation_state: ImplementationState::Native,
                    enabled_default: true,
                    selected: true,
                    policy_order: 1,
                    config: json!({"enabled": true}),
                    compliance_metadata: json!({}),
                    dependencies: json!([]),
                    opaque_xml: None,
                    source_mappings: vec![XccdfSourceMapping {
                        object_kind: "policy".into(),
                        source_identity: "stig://1234".into(),
                        fidelity: "high".into(),
                    }],
                },
                XccdfPolicyExport {
                    policy_id: p2_id,
                    policy_version_id: p2_vid,
                    version: "2.0".into(),
                    publication_state: PublicationState::Draft,
                    semantic_digest: "digest2".into(),
                    name: "CVE Threshold".into(),
                    description: None,
                    policy_type: "cve_threshold".into(),
                    execution_phase: "deployment".into(),
                    implementation_state: ImplementationState::External,
                    enabled_default: true,
                    selected: true,
                    policy_order: 2,
                    config: json!({
                        "no_scan_behavior": "block",
                        "allow_justifications": false,
                        "require_acknowledgment": true,
                        "thresholds": [{"severity": "critical", "max": 0, "action": "block"}]
                    }),
                    compliance_metadata: json!({}),
                    dependencies: json!(["dep1"]),
                    opaque_xml: None,
                    source_mappings: vec![],
                },
            ],
        }
    }

    #[test]
    fn produces_well_formed_xml() {
        let xml = write_bundle_xccdf_export(&test_snapshot()).unwrap();
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(xml.contains("<Benchmark"));
        assert!(xml.contains("</Benchmark>"));
        assert!(xml.contains("</Profile>"));
        assert!(xml.contains("</Group>"));
        assert!(xml.contains("</Rule>"));
        assert!(xml.contains("</metadata>"));
    }

    #[test]
    fn benchmark_id_matches_snapshot() {
        let snap = test_snapshot();
        let expected = snap.benchmark_id();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            xml.contains(&expected),
            "Expected benchmark id {expected} in XML"
        );
    }

    #[test]
    fn profile_has_selects_for_all_policies() {
        let snap = test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        for policy in &snap.policies {
            let rid = policy.rule_id();
            assert!(
                xml.contains(&format!("idref=\"{rid}\"")),
                "Missing select for rule {rid}"
            );
        }
    }

    #[test]
    fn groups_policies_by_type_btree_order() {
        let xml = write_bundle_xccdf_export(&test_snapshot()).unwrap();
        assert!(
            xml.contains("xccdf_org.crystalforge.group:require-cf-agent"),
            "Missing require-cf-agent group"
        );
        assert!(
            xml.contains("xccdf_org.crystalforge.group:cve-threshold"),
            "Missing cve-threshold group"
        );
        let cve_pos = xml.find("cve-threshold").unwrap();
        let cf_pos = xml.find("require-cf-agent").unwrap();
        assert!(
            cve_pos < cf_pos,
            "Groups should be in BTreeMap alphabetical order (cve < require)"
        );
    }

    #[test]
    fn empty_snapshot_produces_benchmark_only() {
        let snap = XccdfBundleExport {
            bundle_id: Uuid::new_v4(),
            bundle_version_id: Uuid::new_v4(),
            version: "0.1".into(),
            publication_state: PublicationState::Incomplete,
            semantic_digest: "empty".into(),
            name: "Empty".into(),
            description: None,
            framework: "none".into(),
            framework_version: None,
            layer: "none".into(),
            owner: "nobody".into(),
            policies: vec![],
        };
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("<Benchmark"));
        assert!(xml.contains("</Benchmark>"));
        assert!(xml.contains("<Profile"));
        assert!(!xml.contains("<Group"));
        assert!(!xml.contains("<Rule"));
        assert!(!xml.contains("<select"));
    }

    #[test]
    fn source_mappings_emitted_when_present() {
        let snap = test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            xml.contains("cf:source-mappings"),
            "Missing source-mappings element"
        );
        assert!(xml.contains("cf:source"), "Missing cf:source element");
        assert!(
            xml.contains("cf:object-kind"),
            "Missing cf:object-kind element"
        );
    }

    fn make_single_policy_snapshot(policies: Vec<XccdfPolicyExport>) -> XccdfBundleExport {
        XccdfBundleExport {
            bundle_id: Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap(),
            bundle_version_id: Uuid::parse_str("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb").unwrap(),
            version: "1.0".into(),
            publication_state: PublicationState::Draft,
            semantic_digest: "digest".into(),
            name: "Test".into(),
            description: None,
            framework: "STIG".into(),
            framework_version: Some("V1R1".into()),
            layer: "os".into(),
            owner: "team".into(),
            policies,
        }
    }

    #[test]
    fn require_cf_agent_policy_type() {
        let pv = test_policy(
            "require_cf_agent",
            ImplementationState::Native,
            json!({"enabled": true}),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:require-crystal-forge-agent"));
        assert!(xml.contains("cf:implementation"));
        assert!(xml.contains("cf:policy"));
        assert!(xml.contains("cf:execution"));
    }

    #[test]
    fn require_packages_policy_type() {
        let pv = test_policy(
            "require_packages",
            ImplementationState::Native,
            json!({"packages": ["nginx", "openssl"]}),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:require-packages"));
        assert!(xml.contains("cf:package"));
        assert!(xml.contains("nginx"));
        assert!(xml.contains("openssl"));
    }

    #[test]
    fn custom_check_single_expression() {
        let pv = test_policy(
            "custom_check",
            ImplementationState::Native,
            json!({
                "expression": "cfg.config.networking.firewall.enable",
                "description": "Firewall enabled",
                "field_name": "firewallEnabled",
                "strict": true,
                "mode": "all"
            }),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:custom-check"));
        assert!(xml.contains("cf:rule"));
        assert!(xml.contains("cf:expression"));
        assert!(xml.contains("language=\"nix\""));
        assert!(xml.contains("cfg.config.networking.firewall.enable"));
        assert!(xml.contains("firewallEnabled"));
    }

    #[test]
    fn custom_check_multi_rule_all() {
        let pv = test_policy(
            "custom_check",
            ImplementationState::Native,
            json!({
                "mode": "all",
                "context": "nix-eval",
                "binding": "config",
                "rules": [
                    {
                        "expression": "a",
                        "description": "Rule A",
                        "field_name": "a",
                        "strict": true
                    },
                    {
                        "expression": "b",
                        "description": "Rule B",
                        "field_name": "b",
                        "strict": false
                    }
                ]
            }),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("mode=\"all\""));
        let rule_count = xml.matches("<cf:rule ").count();
        assert_eq!(rule_count, 2, "Expected 2 cf:rule elements");
        assert!(xml.contains("field-name=\"a\""));
        assert!(xml.contains("field-name=\"b\""));
    }

    #[test]
    fn custom_check_multi_rule_any() {
        let pv = test_policy(
            "custom_check",
            ImplementationState::Native,
            json!({
                "mode": "any",
                "context": "nix-eval",
                "binding": "config",
                "rules": [
                    {
                        "expression": "x",
                        "field_name": "x",
                        "strict": true
                    }
                ]
            }),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("mode=\"any\""));
    }

    #[test]
    fn require_cve_check_policy_type() {
        let pv = test_policy(
            "require_cve_check",
            ImplementationState::Native,
            json!({
                "max_critical": 0,
                "require_high_justification": true,
                "when_no_scan": "block",
                "max_high": 5
            }),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:require-cve-check"));
        assert!(xml.contains("max-critical=\"0\""));
        assert!(xml.contains("require-high-justification=\"true\""));
        assert!(xml.contains("when-no-scan=\"block\""));
        assert!(xml.contains("cf:max-high"));
    }

    #[test]
    fn time_window_policy_type() {
        let pv = test_policy(
            "time_window",
            ImplementationState::Native,
            json!({
                "start_time": "09:00",
                "end_time": "17:00",
                "timezone": "UTC",
                "action": "block",
                "description": "Business hours",
                "days": ["mon", "tue", "wed"]
            }),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:time-window"));
        assert!(xml.contains("start-time=\"09:00\""));
        assert!(xml.contains("end-time=\"17:00\""));
        assert!(xml.contains("timezone=\"UTC\""));
        assert!(xml.contains("action=\"block\""));
        assert!(xml.contains("cf:day"));
        assert!(xml.contains("mon"));
        assert!(xml.contains("tue"));
        assert!(xml.contains("wed"));
    }

    #[test]
    fn require_approvals_policy_type() {
        let pv = test_policy(
            "require_approvals",
            ImplementationState::Native,
            json!({
                "count": 2,
                "role": "admin",
                "distinct": true,
                "description": "Two admins required",
                "expires_after_hours": 24
            }),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:require-approvals"));
        assert!(xml.contains("count=\"2\""));
        assert!(xml.contains("role=\"admin\""));
        assert!(xml.contains("distinct=\"true\""));
        assert!(xml.contains("cf:expires-after-hours"));
        assert!(xml.contains("24"));
    }

    #[test]
    fn canary_rollout_policy_type() {
        let pv = test_policy(
            "canary_rollout",
            ImplementationState::Native,
            json!({
                "percentage": 25,
                "observe_duration_minutes": 30,
                "selection_strategy": "random",
                "description": "Gradual rollout",
                "health_check": {
                    "type": "systemd",
                    "fail_threshold": 3
                }
            }),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:canary-rollout"));
        assert!(xml.contains("percentage=\"25\""));
        assert!(xml.contains("observe-duration-minutes=\"30\""));
        assert!(xml.contains("selection-strategy=\"random\""));
        assert!(xml.contains("cf:health-check"));
        assert!(xml.contains("type=\"systemd\""));
        assert!(xml.contains("fail-threshold=\"3\""));
    }

    #[test]
    fn cve_threshold_policy_type() {
        let pv = test_policy(
            "cve_threshold",
            ImplementationState::Native,
            json!({
                "no_scan_behavior": "block",
                "allow_justifications": false,
                "require_acknowledgment": true,
                "thresholds": [
                    {"severity": "critical", "max": 0, "action": "block"},
                    {"severity": "high", "max": 3, "action": "warn"}
                ]
            }),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:cve-threshold"));
        assert!(xml.contains("no-scan-behavior=\"block\""));
        assert!(xml.contains("allow-justifications=\"false\""));
        assert!(xml.contains("require-acknowledgment=\"true\""));
        assert!(xml.contains("cf:threshold"));
        assert!(xml.contains("severity=\"critical\""));
        assert!(xml.contains("severity=\"high\""));
        assert!(xml.contains("max=\"0\""));
        assert!(xml.contains("max=\"3\""));
    }

    #[test]
    fn native_policies_emit_cf_check_and_fix() {
        let pv = test_policy(
            "require_cf_agent",
            ImplementationState::Native,
            json!({"enabled": true}),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            xml.contains(&format!("system=\"{CF_POLICY_CHECK_SYSTEM}\"")),
            "Missing CF check system"
        );
        assert!(
            xml.contains("check-content-ref"),
            "Missing check-content-ref"
        );
        assert!(
            xml.contains(&format!("system=\"{CF_NIX_FIX_SYSTEM}\"")),
            "Missing CF fix system"
        );
        assert!(xml.contains("<fix"), "Missing fix element");
    }

    #[test]
    fn manual_policy_emits_explanatory_check_text() {
        let pv = test_policy(
            "require_cf_agent",
            ImplementationState::Manual,
            json!({}),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            xml.contains("user must provide evidence"),
            "Missing manual policy check text"
        );
        assert!(
            !xml.contains("check-content-ref"),
            "Manual should not have check-content-ref"
        );
        assert!(
            !xml.contains("fix"),
            "Manual should not have fix element"
        );
    }

    #[test]
    fn unbound_policy_emits_explanatory_check_text() {
        let pv = test_policy(
            "require_cf_agent",
            ImplementationState::Unbound,
            json!({}),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            xml.contains("no implementation"),
            "Missing unbound policy check text"
        );
    }

    #[test]
    fn external_policy_emits_explanatory_check_text() {
        let pv = test_policy(
            "require_cf_agent",
            ImplementationState::External,
            json!({}),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            xml.contains("external system"),
            "Missing external policy check text"
        );
    }

    #[test]
    fn opaque_policy_emits_explanatory_check_text() {
        let pv = test_policy(
            "require_cf_agent",
            ImplementationState::Opaque,
            json!({}),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            xml.contains("cannot model check"),
            "Missing opaque policy check text"
        );
    }

    #[test]
    fn policy_identity_element_with_uuids_and_digest() {
        let pv = test_policy(
            "require_cf_agent",
            ImplementationState::Native,
            json!({}),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:policy-identity"));
        assert!(xml.contains("policy-id=\"urn:uuid:"));
        assert!(xml.contains("policy-version-id=\"urn:uuid:"));
        assert!(xml.contains("cf:policy-version"));
        assert!(xml.contains("cf:content-digest"));
        assert!(xml.contains("algorithm=\"sha-256\""));
        assert!(xml.contains("abc123"));
    }

    #[test]
    fn xml_escaping_of_special_chars() {
        let pv = XccdfPolicyExport {
            policy_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            policy_version_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
            version: "1.0".into(),
            publication_state: PublicationState::Draft,
            semantic_digest: "digest".into(),
            name: "Policy <with> &special \"chars\"".into(),
            description: Some("Desc <html> & entities".into()),
            policy_type: "require_cf_agent".into(),
            execution_phase: "nix-evaluation".into(),
            implementation_state: ImplementationState::Native,
            enabled_default: true,
            selected: true,
            policy_order: 1,
            config: json!({}),
            compliance_metadata: json!({}),
            dependencies: json!([]),
            opaque_xml: None,
            source_mappings: vec![],
        };
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            !xml.contains("<with>"),
            "Angle bracket should be escaped"
        );
        assert!(
            !xml.contains("&special"),
            "Ampersand should be escaped"
        );
        assert!(xml.contains("&lt;with&gt;"), "Escaped angle brackets");
        assert!(xml.contains("&amp;special"), "Escaped ampersand");
    }

    #[test]
    fn literal_cdata_closing_in_config_does_not_break_xml() {
        let pv = XccdfPolicyExport {
            policy_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            policy_version_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
            version: "1.0".into(),
            publication_state: PublicationState::Draft,
            semantic_digest: "digest".into(),
            name: "CDATA Test".into(),
            description: Some("Contains ]]>evil text".into()),
            policy_type: "require_cf_agent".into(),
            execution_phase: "nix-evaluation".into(),
            implementation_state: ImplementationState::Native,
            enabled_default: true,
            selected: true,
            policy_order: 1,
            config: json!({}),
            compliance_metadata: json!({}),
            dependencies: json!([]),
            opaque_xml: None,
            source_mappings: vec![],
        };
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            !xml.contains("]]>"),
            "Literal ]]> should not appear unescaped"
        );
        assert!(xml.contains("</Benchmark>"), "XML should be well-formed");
    }

    #[test]
    fn standard_ident_elements_from_compliance_metadata() {
        let pv = XccdfPolicyExport {
            policy_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            policy_version_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
            version: "1.0".into(),
            publication_state: PublicationState::Draft,
            semantic_digest: "digest".into(),
            name: "STIG Rule".into(),
            description: None,
            policy_type: "require_cf_agent".into(),
            execution_phase: "nix-evaluation".into(),
            implementation_state: ImplementationState::Native,
            enabled_default: true,
            selected: true,
            policy_order: 1,
            config: json!({}),
            compliance_metadata: json!({
                "identifiers": [
                    {"system": "http://cyber.mil/cci", "value": "CCI-002322"},
                    {"system": "http://example.com/ids", "value": "EX-001"}
                ]
            }),
            dependencies: json!([]),
            opaque_xml: None,
            source_mappings: vec![],
        };
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("system=\"http://cyber.mil/cci\""));
        assert!(xml.contains("CCI-002322"));
        assert!(xml.contains("system=\"http://example.com/ids\""));
        assert!(xml.contains("EX-001"));
        let ident_count = xml.matches("<ident ").count();
        assert_eq!(ident_count, 2, "Expected 2 ident elements");
    }

    #[test]
    fn standard_reference_elements_from_compliance_metadata() {
        let pv = XccdfPolicyExport {
            policy_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            policy_version_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
            version: "1.0".into(),
            publication_state: PublicationState::Draft,
            semantic_digest: "digest".into(),
            name: "STIG Rule".into(),
            description: None,
            policy_type: "require_cf_agent".into(),
            execution_phase: "nix-evaluation".into(),
            implementation_state: ImplementationState::Native,
            enabled_default: true,
            selected: true,
            policy_order: 1,
            config: json!({}),
            compliance_metadata: json!({
                "references": [
                    {"href": "https://example.com/stig/V1R1", "title": "STIG V1R1"},
                    {"href": "https://example.com/blank", "title": ""}
                ]
            }),
            dependencies: json!([]),
            opaque_xml: None,
            source_mappings: vec![],
        };
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("href=\"https://example.com/stig/V1R1\""));
        assert!(xml.contains("STIG V1R1"));
        assert!(xml.contains("href=\"https://example.com/blank\""));
        let ref_count = xml.matches("<reference ").count();
        assert_eq!(ref_count, 2, "Expected 2 reference elements");
    }

    #[test]
    fn implementation_state_metadata_element() {
        let snap = test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:policy"));
        assert!(xml.contains("cf:execution"));
        assert!(xml.contains("cf:implementation"));
    }

    #[test]
    fn execution_phase_metadata_element() {
        let pv = test_policy(
            "require_cf_agent",
            ImplementationState::Native,
            json!({}),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:execution"));
        assert!(xml.contains("phase=\"nix-evaluation\""));
        assert!(xml.contains("strict=\"true\""));
    }

    #[test]
    fn bundle_metadata_contains_ids() {
        let snap = test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:bundle-id"));
        assert!(xml.contains("cf:bundle-version-id"));
        let bundle_urn = format!("urn:uuid:{}", snap.bundle_id);
        assert!(xml.contains(&bundle_urn));
        let version_urn = format!("urn:uuid:{}", snap.bundle_version_id);
        assert!(xml.contains(&version_urn));
    }

    #[test]
    fn bundle_metadata_contains_framework_info() {
        let snap = test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:bundle"));
        assert!(xml.contains("cf:framework"));
        assert!(xml.contains("name=\"STIG\""));
        assert!(xml.contains("version=\"V1R1\""));
        assert!(xml.contains("cf:layer"));
        assert!(xml.contains("cf:owner"));
        assert!(xml.contains("cf:semantic-digest"));
        assert!(xml.contains("cf:publication-state"));
    }

    #[test]
    fn opaque_xml_preserved() {
        let pv = XccdfPolicyExport {
            policy_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            policy_version_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
            version: "1.0".into(),
            publication_state: PublicationState::Draft,
            semantic_digest: "digest".into(),
            name: "Opaque Rule".into(),
            description: None,
            policy_type: "require_cf_agent".into(),
            execution_phase: "nix-evaluation".into(),
            implementation_state: ImplementationState::Opaque,
            enabled_default: true,
            selected: true,
            policy_order: 1,
            config: json!({}),
            compliance_metadata: json!({}),
            dependencies: json!([]),
            opaque_xml: Some("<custom:data>value</custom:data>".into()),
            source_mappings: vec![],
        };
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:opaque-xml"));
        assert!(xml.contains("custom:data"));
    }

    #[test]
    fn per_rule_source_mappings() {
        let pv = XccdfPolicyExport {
            policy_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            policy_version_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
            version: "1.0".into(),
            publication_state: PublicationState::Draft,
            semantic_digest: "digest".into(),
            name: "Mapped Rule".into(),
            description: None,
            policy_type: "require_cf_agent".into(),
            execution_phase: "nix-evaluation".into(),
            implementation_state: ImplementationState::Native,
            enabled_default: true,
            selected: true,
            policy_order: 1,
            config: json!({}),
            compliance_metadata: json!({}),
            dependencies: json!([]),
            opaque_xml: None,
            source_mappings: vec![XccdfSourceMapping {
                object_kind: "rule".into(),
                source_identity: "stig://V1R1/RULE-001".into(),
                fidelity: "exact".into(),
            }],
        };
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        let occurrences = xml.matches("cf:source-mappings").count();
        assert!(
            occurrences >= 2,
            "Expected bundle-level and per-rule source-mappings, got {occurrences}"
        );
    }

    #[test]
    fn dependencies_emitted_when_nonempty() {
        let pv = test_policy(
            "require_cf_agent",
            ImplementationState::Native,
            json!({}),
        );
        let mut pv = pv;
        pv.dependencies = json!(["nixos/modules/services/nginx.nix"]);
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(xml.contains("cf:dependencies"));
        assert!(xml.contains("cf:nix-option"));
        assert!(xml.contains("nixos/modules/services/nginx.nix"));
    }

    #[test]
    fn no_dependencies_when_empty_array() {
        let pv = test_policy(
            "require_cf_agent",
            ImplementationState::Native,
            json!({}),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            !xml.contains("cf:dependencies"),
            "Empty dependencies array should not emit cf:dependencies"
        );
    }

    #[test]
    fn custom_check_expression_escaping() {
        let pv = test_policy(
            "custom_check",
            ImplementationState::Native,
            json!({
                "expression": "a < b && c > d",
                "description": "Test & escaping",
                "field_name": "test",
                "strict": true,
                "mode": "all"
            }),
        );
        let snap = make_single_policy_snapshot(vec![pv]);
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        assert!(
            !xml.contains("a < b && c > d"),
            "Expression should be XML-escaped"
        );
        assert!(xml.contains("a &lt; b &amp;&amp; c &gt; d"));
        assert!(xml.contains("Test &amp; escaping"));
    }

    #[test]
    fn severity_mapping_for_all_types() {
        let high_types = ["require_cve_check", "require_approvals", "cve_threshold"];
        let medium_types = [
            "require_cf_agent",
            "require_packages",
            "custom_check",
            "time_window",
            "canary_rollout",
        ];
        for t in &high_types {
            assert_eq!(severity_for_type(t), "high", "{t} should be high severity");
        }
        for t in &medium_types {
            assert_eq!(
                severity_for_type(t),
                "medium",
                "{t} should be medium severity"
            );
        }
    }

    // ── Round-trip tests ───────────────────────────────────────────────────────

    use super::super::parser::parse_xccdf;
    use super::super::models::DocumentClass;
    use crate::compliance::interchange::InterchangeLimits;

    fn full_test_snapshot() -> XccdfBundleExport {
        let bundle_id = Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap();
        let bundle_version_id =
            Uuid::parse_str("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb").unwrap();

        let p1_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let p1_vid = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
        let p2_id = Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap();
        let p2_vid = Uuid::parse_str("44444444-4444-4444-4444-444444444444").unwrap();
        let p3_id = Uuid::parse_str("55555555-5555-5555-5555-555555555555").unwrap();
        let p3_vid = Uuid::parse_str("66666666-6666-6666-6666-666666666666").unwrap();
        let p4_id = Uuid::parse_str("77777777-7777-7777-7777-777777777777").unwrap();
        let p4_vid = Uuid::parse_str("88888888-8888-8888-8888-888888888888").unwrap();

        XccdfBundleExport {
            bundle_id,
            bundle_version_id,
            version: "1.0".into(),
            publication_state: PublicationState::Draft,
            semantic_digest: "abc123".into(),
            name: "Test Bundle".into(),
            description: Some("A test bundle for round-trip".into()),
            framework: "STIG".into(),
            framework_version: Some("V1R1".into()),
            layer: "os".into(),
            owner: "Team".into(),
            policies: vec![
                // CF agent policy – native, nix-evaluation
                XccdfPolicyExport {
                    policy_id: p1_id,
                    policy_version_id: p1_vid,
                    version: "1.0".into(),
                    publication_state: PublicationState::Accepted,
                    semantic_digest: "digest1".into(),
                    name: "CF Agent Policy".into(),
                    description: Some("Requires CF agent".into()),
                    policy_type: "require_cf_agent".into(),
                    execution_phase: "nix-evaluation".into(),
                    implementation_state: ImplementationState::Native,
                    enabled_default: true,
                    selected: true,
                    policy_order: 1,
                    config: json!({"enabled": true}),
                    compliance_metadata: json!({}),
                    dependencies: json!([]),
                    opaque_xml: None,
                    source_mappings: vec![XccdfSourceMapping {
                        object_kind: "policy".into(),
                        source_identity: "stig://1234".into(),
                        fidelity: "high".into(),
                    }],
                },
                // custom_check policy – native, multi-rule with all mode
                XccdfPolicyExport {
                    policy_id: p2_id,
                    policy_version_id: p2_vid,
                    version: "1.0".into(),
                    publication_state: PublicationState::Draft,
                    semantic_digest: "digest2".into(),
                    name: "Custom Check Policy".into(),
                    description: Some("Multi-rule custom check".into()),
                    policy_type: "custom_check".into(),
                    execution_phase: "nix-evaluation".into(),
                    implementation_state: ImplementationState::Native,
                    enabled_default: true,
                    selected: true,
                    policy_order: 2,
                    config: json!({
                        "mode": "all",
                        "context": "nix-eval",
                        "binding": "config",
                        "rules": [
                            {
                                "expression": "cfg.config.networking.firewall.enable",
                                "description": "Firewall enabled",
                                "field_name": "firewallEnabled",
                                "strict": true
                            },
                            {
                                "expression": "cfg.config.services.openssh.enable",
                                "description": "SSH enabled",
                                "field_name": "sshEnabled",
                                "strict": false
                            }
                        ]
                    }),
                    compliance_metadata: json!({
                        "identifiers": [
                            {"system": "http://cyber.mil/cci", "value": "CCI-002322"}
                        ]
                    }),
                    dependencies: json!([]),
                    opaque_xml: None,
                    source_mappings: vec![],
                },
                // cve_threshold policy – external, deployment-time
                XccdfPolicyExport {
                    policy_id: p3_id,
                    policy_version_id: p3_vid,
                    version: "2.0".into(),
                    publication_state: PublicationState::Draft,
                    semantic_digest: "digest3".into(),
                    name: "CVE Threshold".into(),
                    description: Some("Blocks deployment if CVEs exceed threshold".into()),
                    policy_type: "cve_threshold".into(),
                    execution_phase: "deployment".into(),
                    implementation_state: ImplementationState::External,
                    enabled_default: true,
                    selected: true,
                    policy_order: 3,
                    config: json!({
                        "no_scan_behavior": "block",
                        "allow_justifications": false,
                        "require_acknowledgment": true,
                        "thresholds": [
                            {"severity": "critical", "max": 0, "action": "block"},
                            {"severity": "high", "max": 3, "action": "warn"}
                        ]
                    }),
                    compliance_metadata: json!({}),
                    dependencies: json!(["dep1"]),
                    opaque_xml: None,
                    source_mappings: vec![],
                },
                // manual policy
                XccdfPolicyExport {
                    policy_id: p4_id,
                    policy_version_id: p4_vid,
                    version: "1.0".into(),
                    publication_state: PublicationState::Incomplete,
                    semantic_digest: "digest4".into(),
                    name: "Manual Review".into(),
                    description: Some("Requires manual review".into()),
                    policy_type: "require_approvals".into(),
                    execution_phase: "post-deployment".into(),
                    implementation_state: ImplementationState::Manual,
                    enabled_default: true,
                    selected: true,
                    policy_order: 4,
                    config: json!({
                        "count": 2,
                        "role": "admin",
                        "distinct": true,
                        "description": "Two admins required"
                    }),
                    compliance_metadata: json!({}),
                    dependencies: json!([]),
                    opaque_xml: None,
                    source_mappings: vec![],
                },
            ],
        }
    }

    /// Round-trip: write → parse → verify CF-native classification.
    #[test]
    fn round_trip_classifies_as_cf_native() {
        let snap = full_test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        let limits = InterchangeLimits::default();
        let parsed = parse_xccdf(xml.as_bytes(), Some("export.xml"), &limits).unwrap();
        assert!(
            matches!(
                parsed.class,
                DocumentClass::CfNativeExact | DocumentClass::CfNativeUnsupportedExtension
            ),
            "Expected CF-native classification, got {:?}",
            parsed.class
        );
    }

    /// Round-trip: benchmark_id (derived from bundle_id) survives.
    #[test]
    fn round_trip_preserves_benchmark_id() {
        let snap = full_test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        let limits = InterchangeLimits::default();
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        let expected_bench_id = snap.benchmark_id();
        let bm = parsed.benchmark.expect("benchmark present");
        assert_eq!(bm.id, expected_bench_id);
    }

    /// Round-trip: benchmark title survives.
    #[test]
    fn round_trip_preserves_benchmark_title() {
        let snap = full_test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        let limits = InterchangeLimits::default();
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        let bm = parsed.benchmark.expect("benchmark present");
        assert_eq!(bm.title.as_deref(), Some("Test Bundle"));
    }

    /// Round-trip: rule count matches.
    #[test]
    fn round_trip_preserves_rule_count() {
        let snap = full_test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        let limits = InterchangeLimits::default();
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        assert_eq!(parsed.rules.len(), snap.policies.len());
    }

    /// Round-trip: CF policy identity metadata is preserved for native rules.
    #[test]
    fn round_trip_preserves_cf_policy_meta() {
        let snap = full_test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        let limits = InterchangeLimits::default();
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        let rules_with_meta: Vec<_> = parsed
            .rules
            .iter()
            .filter(|r| r.cf_policy_meta.is_some())
            .collect();
        assert!(
            !rules_with_meta.is_empty(),
            "At least one rule should have cf_policy_meta"
        );
        let meta = rules_with_meta[0].cf_policy_meta.as_ref().unwrap();
        let expected_policy = snap
            .policies
            .iter()
            .find(|p| p.policy_id == meta.policy_id)
            .expect("matching policy in snapshot");
        assert_eq!(meta.policy_id, expected_policy.policy_id);
        assert_eq!(meta.policy_version_id, expected_policy.policy_version_id);
    }

    /// Round-trip: standard ident elements are preserved.
    #[test]
    fn round_trip_preserves_identifiers() {
        let snap = full_test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        let limits = InterchangeLimits::default();
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        let rule = parsed
            .rules
            .iter()
            .find(|r| !r.identifiers.is_empty())
            .expect("rule with identifiers");
        let ident = rule
            .identifiers
            .iter()
            .find(|i| i.system == "http://cyber.mil/cci")
            .expect("CCI ident");
        assert_eq!(ident.value, "CCI-002322");
    }

    /// Round-trip: profile with selects survives.
    #[test]
    fn round_trip_preserves_profile_selects() {
        let snap = full_test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        let limits = InterchangeLimits::default();
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        assert!(!parsed.profiles.is_empty(), "Profile should be present");
        let profile = &parsed.profiles[0];
        assert_eq!(profile.select_ids.len(), snap.policies.len());
    }

    /// Round-trip: no blocking errors on well-formed input.
    #[test]
    fn round_trip_has_no_blocking_errors() {
        let snap = full_test_snapshot();
        let xml = write_bundle_xccdf_export(&snap).unwrap();
        let limits = InterchangeLimits::default();
        let parsed = parse_xccdf(xml.as_bytes(), None, &limits).unwrap();
        let blocking: Vec<_> = parsed.errors.iter().filter(|e| e.blocking).collect();
        assert!(
            blocking.is_empty(),
            "No blocking errors expected, got: {blocking:?}"
        );
    }
}
