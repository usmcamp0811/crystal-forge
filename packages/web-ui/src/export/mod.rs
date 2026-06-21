//! Client-side evidence export generators.
//!
//! All formats are built in WASM from the data already fetched by the UI and
//! then triggered as browser downloads via `URL.createObjectURL`.  No extra
//! server round-trips are required beyond the evidence fetch that happens at
//! download time.
//!
//! Supported formats
//! -----------------
//! * **Crystal Forge JSON** — canonical serialisation of bundle + all system
//!   evidence; best for re-ingestion or custom dashboards.
//! * **CSV** — flat per-(host, control) table for spreadsheet consumers.
//! * **SARIF 2.1.0** — Static Analysis Results Interchange Format used by
//!   GitHub, GitLab, and most SAST/posture tools.
//! * **OSCAL 1.1.2 Assessment Results** — NIST format used for ATO packages.
//! * **PDF (HTML print)** — opens a styled print window; the browser renders
//!   it to PDF via Ctrl-P / "Save as PDF".  No server-side PDF library needed.

use crate::api::models::{
    ComplianceBundleSummary, ComplianceControlEvidence, ComplianceControlStatus,
    ComplianceEvidenceResponse, ComplianceRollupTotals, ComplianceSystemRollup,
};

// ─── Trigger browser download ─────────────────────────────────────────────────

/// Create a Blob from `content`, attach it to an invisible `<a>` and click it.
/// Returns `Err` if any Web API call fails.
pub fn trigger_download(filename: &str, mime: &str, content: &str) -> Result<(), String> {
    use js_sys::Array;
    use wasm_bindgen::JsCast;
    use web_sys::{BlobPropertyBag, HtmlAnchorElement, Url};

    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;

    // Build Blob
    let parts = Array::new();
    parts.push(&wasm_bindgen::JsValue::from_str(content));
    let mut opts = BlobPropertyBag::new();
    opts.type_(mime);
    let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &opts)
        .map_err(|_| "blob creation failed")?;

    // Object URL
    let url = Url::create_object_url_with_blob(&blob).map_err(|_| "createObjectURL failed")?;

    // Invisible anchor click
    let anchor: HtmlAnchorElement = document
        .create_element("a")
        .map_err(|_| "createElement failed")?
        .dyn_into()
        .map_err(|_| "dyn_into anchor failed")?;
    anchor.set_href(&url);
    anchor.set_download(filename);
    anchor.set_attribute("style", "display:none").ok();
    document
        .body()
        .ok_or("no body")?
        .append_child(&anchor)
        .map_err(|_| "append failed")?;
    anchor.click();
    anchor
        .parent_node()
        .and_then(|p| p.remove_child(&anchor).ok());
    Url::revoke_object_url(&url).ok();

    Ok(())
}

// ─── Crystal Forge JSON ───────────────────────────────────────────────────────

pub struct ExportPayload<'a> {
    pub bundle: &'a ComplianceBundleSummary,
    pub totals: &'a ComplianceRollupTotals,
    pub systems: &'a [ComplianceSystemRollup],
    pub evidence: &'a [ComplianceEvidenceResponse],
    pub include_waivers: bool,
    pub include_source: bool,
    pub scope: &'a str, // "all" | "fail" | "clean"
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
        let scoped_ids: std::collections::HashSet<uuid::Uuid> =
            self.scoped_systems().iter().map(|s| s.system_id).collect();
        self.evidence
            .iter()
            .filter(|e| scoped_ids.contains(&e.system_id))
            .collect()
    }
}

/// Crystal Forge native JSON — full fidelity, re-ingestable.
pub fn build_cf_json(p: &ExportPayload<'_>) -> String {
    // We lean on serde_json directly since all types derive Serialize.
    use serde_json::{Map, Value, json};

    let systems_arr: Vec<Value> = p
        .scoped_systems()
        .iter()
        .map(|s| {
            json!({
                "system_id": s.system_id,
                "hostname":  s.hostname,
                "environment": s.environment,
                "score": s.score,
                "pass": s.pass,
                "warn": s.warn,
                "fail": s.fail,
                "waiver": s.waiver,
                "total": s.total,
            })
        })
        .collect();

    let evidence_arr: Vec<Value> = p
        .scoped_evidence()
        .iter()
        .map(|ev| {
            let controls: Vec<Value> = ev
                .controls
                .iter()
                .filter(|c| {
                    p.include_waivers
                        || !matches!(c.status, ComplianceControlStatus::Waiver)
                })
                .map(|c| {
                    let mut obj = Map::new();
                    obj.insert("policy_id".into(), json!(c.policy_id));
                    obj.insert("policy_name".into(), json!(c.policy_name));
                    obj.insert("status".into(), json!(format!("{:?}", c.status).to_lowercase()));
                    obj.insert("severity".into(), json!(c.severity));
                    obj.insert("summary".into(), json!(c.summary));
                    obj.insert("framework_mapping".into(), json!(c.framework_mapping));
                    if p.include_source {
                        let items: Vec<Value> = c
                            .evidence_items
                            .iter()
                            .map(|i| {
                                json!({
                                    "kind": i.kind,
                                    "label": i.label,
                                    "body": i.body,
                                })
                            })
                            .collect();
                        obj.insert("evidence_items".into(), Value::Array(items));
                    }
                    Value::Object(obj)
                })
                .collect();
            json!({
                "system_id": ev.system_id,
                "hostname":  ev.hostname,
                "controls":  controls,
            })
        })
        .collect();

    let root = json!({
        "crystal_forge_export": {
            "schema_version": "1.0",
            "generated_at": js_sys::Date::new_0().to_iso_string().as_string().unwrap_or_default(),
            "bundle": {
                "id":          p.bundle.id,
                "name":        p.bundle.name,
                "framework":   p.bundle.framework,
                "version":     p.bundle.version,
                "layer":       p.bundle.layer,
                "owner":       p.bundle.owner,
                "description": p.bundle.description,
            },
            "totals": {
                "system_count":          p.totals.system_count,
                "fully_compliant_count": p.totals.fully_compliant_count,
                "overall_score":         p.totals.overall_score,
                "pass":   p.totals.pass,
                "warn":   p.totals.warn,
                "fail":   p.totals.fail,
                "waiver": p.totals.waiver,
                "total_controls": p.totals.total_controls,
            },
            "systems":  systems_arr,
            "evidence": evidence_arr,
        }
    });

    serde_json::to_string_pretty(&root).unwrap_or_default()
}

// ─── CSV ──────────────────────────────────────────────────────────────────────

/// Flat per-(host, control) CSV for spreadsheet consumers.
pub fn build_csv(p: &ExportPayload<'_>) -> String {
    let mut out = String::new();
    out.push_str(
        "bundle_name,bundle_framework,bundle_version,hostname,environment,\
         control_name,status,severity,score,pass,warn,fail,waiver,summary,framework_mapping\n",
    );

    let scoped_ev = p.scoped_evidence();

    if scoped_ev.is_empty() {
        // Fall back to rollup-only rows (no per-control detail)
        for s in p.scoped_systems() {
            csv_row(
                &mut out,
                &[
                    &p.bundle.name,
                    &p.bundle.framework,
                    &p.bundle.version,
                    &s.hostname,
                    s.environment.as_deref().unwrap_or(""),
                    "(rollup only)",
                    "—",
                    "—",
                    &s.score.to_string(),
                    &s.pass.to_string(),
                    &s.warn.to_string(),
                    &s.fail.to_string(),
                    &s.waiver.to_string(),
                    "",
                    "",
                ],
            );
        }
        return out;
    }

    for ev in &scoped_ev {
        // Find matching rollup for score columns
        let rollup = p.systems.iter().find(|s| s.system_id == ev.system_id);
        for ctrl in &ev.controls {
            if !p.include_waivers && matches!(ctrl.status, ComplianceControlStatus::Waiver) {
                continue;
            }
            let status_str = status_label(&ctrl.status);
            csv_row(
                &mut out,
                &[
                    &p.bundle.name,
                    &p.bundle.framework,
                    &p.bundle.version,
                    &ev.hostname,
                    rollup
                        .and_then(|r| r.environment.as_deref())
                        .unwrap_or(""),
                    &ctrl.policy_name,
                    status_str,
                    &ctrl.severity,
                    &rollup.map(|r| r.score.to_string()).unwrap_or_default(),
                    &rollup.map(|r| r.pass.to_string()).unwrap_or_default(),
                    &rollup.map(|r| r.warn.to_string()).unwrap_or_default(),
                    &rollup.map(|r| r.fail.to_string()).unwrap_or_default(),
                    &rollup.map(|r| r.waiver.to_string()).unwrap_or_default(),
                    &ctrl.summary,
                    &ctrl.framework_mapping,
                ],
            );
        }
    }
    out
}

fn csv_row(out: &mut String, fields: &[&str]) {
    let row: Vec<String> = fields
        .iter()
        .map(|f| {
            // RFC 4180: quote fields containing comma, quote, or newline
            if f.contains(',') || f.contains('"') || f.contains('\n') {
                format!("\"{}\"", f.replace('"', "\"\""))
            } else {
                f.to_string()
            }
        })
        .collect();
    out.push_str(&row.join(","));
    out.push('\n');
}

// ─── SARIF 2.1.0 ─────────────────────────────────────────────────────────────

/// SARIF 2.1.0 — one `run` per bundle, one `result` per (system × control).
/// Maps: tool=Crystal Forge, rules=controls, results=findings.
pub fn build_sarif(p: &ExportPayload<'_>) -> String {
    use serde_json::{json, Value};

    let scoped_ev = p.scoped_evidence();

    // Rules = unique controls (by policy_id)
    let rules: Vec<Value> = {
        let mut seen = std::collections::HashSet::new();
        let mut rules = Vec::new();
        for ev in &scoped_ev {
            for ctrl in &ev.controls {
                if seen.insert(ctrl.policy_id) {
                    rules.push(json!({
                        "id": ctrl.policy_id.to_string(),
                        "name": ctrl.policy_name,
                        "shortDescription": { "text": ctrl.policy_name },
                        "fullDescription": { "text": ctrl.summary },
                        "helpUri": "",
                        "properties": {
                            "tags": ["compliance", &p.bundle.framework],
                            "frameworkMapping": ctrl.framework_mapping,
                        }
                    }));
                }
            }
        }
        rules
    };

    // Results = one per (system × control)
    let results: Vec<Value> = scoped_ev
        .iter()
        .flat_map(|ev| {
            ev.controls.iter().filter_map(|ctrl| {
                if !p.include_waivers && matches!(ctrl.status, ComplianceControlStatus::Waiver) {
                    return None;
                }
                let sarif_level = match ctrl.status {
                    ComplianceControlStatus::Pass => "none",
                    ComplianceControlStatus::Warn | ComplianceControlStatus::Waiver => "warning",
                    ComplianceControlStatus::Fail => "error",
                };
                let sarif_kind = match ctrl.status {
                    ComplianceControlStatus::Pass => "pass",
                    ComplianceControlStatus::Waiver => "open",
                    _ => "fail",
                };
                Some(json!({
                    "ruleId": ctrl.policy_id.to_string(),
                    "kind": sarif_kind,
                    "level": sarif_level,
                    "message": { "text": ctrl.summary },
                    "locations": [{
                        "logicalLocations": [{
                            "name": ev.hostname,
                            "kind": "machine",
                            "properties": {
                                "environment": p.systems.iter()
                                    .find(|s| s.system_id == ev.system_id)
                                    .and_then(|s| s.environment.as_deref())
                                    .unwrap_or("unknown"),
                            }
                        }]
                    }],
                    "properties": {
                        "severity": ctrl.severity,
                        "frameworkMapping": ctrl.framework_mapping,
                        "bundleName": p.bundle.name,
                        "bundleVersion": p.bundle.version,
                    }
                }))
            })
        })
        .collect();

    let doc = json!({
        "version": "2.1.0",
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "Crystal Forge",
                    "version": "0.3.0",
                    "informationUri": "",
                    "rules": rules,
                }
            },
            "automationDetails": {
                "id": format!("{}/{}", p.bundle.id, js_sys::Date::new_0().to_iso_string().as_string().unwrap_or_default()),
                "description": { "text": format!("{} v{} compliance assessment", p.bundle.name, p.bundle.version) },
            },
            "results": results,
            "properties": {
                "bundle": p.bundle.name,
                "framework": p.bundle.framework,
                "version": p.bundle.version,
                "overallScore": p.totals.overall_score,
                "totalHosts": p.totals.system_count,
                "compliantHosts": p.totals.fully_compliant_count,
            }
        }]
    });

    serde_json::to_string_pretty(&doc).unwrap_or_default()
}

// ─── OSCAL 1.1.2 Assessment Results ──────────────────────────────────────────

/// OSCAL 1.1.2 Assessment Results (JSON).
/// Produces a minimal but valid AR document from the rollup + evidence data.
pub fn build_oscal(p: &ExportPayload<'_>) -> String {
    use serde_json::{json, Value};

    let now = js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default();
    let ar_uuid = uuid::Uuid::new_v4().to_string();
    let ssp_uuid = p.bundle.id.to_string();

    let scoped_ev = p.scoped_evidence();

    // Components = one per host
    let components: Vec<Value> = p
        .scoped_systems()
        .iter()
        .map(|s| {
            json!({
                "uuid": s.system_id.to_string(),
                "type": "software",
                "title": s.hostname,
                "description": format!("Host {} in environment {}", s.hostname, s.environment.as_deref().unwrap_or("unknown")),
                "status": { "state": if s.fail == 0 { "operational" } else { "under-development" } }
            })
        })
        .collect();

    // Observations = one per (system × control evidence item)
    let mut observations: Vec<Value> = Vec::new();
    let mut findings: Vec<Value> = Vec::new();

    for ev in &scoped_ev {
        let rollup = p.systems.iter().find(|s| s.system_id == ev.system_id);
        for ctrl in &ev.controls {
            if !p.include_waivers && matches!(ctrl.status, ComplianceControlStatus::Waiver) {
                continue;
            }
            let obs_uuid = uuid::Uuid::new_v4().to_string();
            let finding_uuid = uuid::Uuid::new_v4().to_string();

            let (oscal_state, oscal_reason) = match ctrl.status {
                ComplianceControlStatus::Pass => ("satisfied", "pass"),
                ComplianceControlStatus::Warn => ("not-satisfied", "other"),
                ComplianceControlStatus::Fail => ("not-satisfied", "fail-adjusted"),
                ComplianceControlStatus::Waiver => ("not-applicable", "accept-risk"),
            };

            // Collect evidence items as relevant evidence
            let relevant_evidence: Vec<Value> = if p.include_source {
                ctrl.evidence_items
                    .iter()
                    .map(|item| {
                        json!({
                            "description": format!("{}: {}", item.label, item.body),
                        })
                    })
                    .collect()
            } else {
                vec![]
            };

            observations.push(json!({
                "uuid": obs_uuid,
                "title": format!("{} — {}", ev.hostname, ctrl.policy_name),
                "description": ctrl.summary,
                "methods": ["AUTOMATED"],
                "types": ["finding"],
                "subjects": [{
                    "subject-uuid": ev.system_id.to_string(),
                    "type": "component",
                    "title": ev.hostname,
                }],
                "relevant-evidence": relevant_evidence,
                "collected": now,
                "props": [{
                    "name": "framework-mapping",
                    "value": ctrl.framework_mapping,
                }, {
                    "name": "severity",
                    "value": ctrl.severity,
                }]
            }));

            findings.push(json!({
                "uuid": finding_uuid,
                "title": format!("{} on {}", ctrl.policy_name, ev.hostname),
                "description": ctrl.summary,
                "target": {
                    "type": "objective-id",
                    "target-id": ctrl.policy_id.to_string(),
                    "title": ctrl.policy_name,
                    "status": {
                        "state": oscal_state,
                        "reason": oscal_reason,
                    }
                },
                "related-observations": [{ "observation-uuid": obs_uuid }],
                "props": [{
                    "name": "hostname",
                    "value": ev.hostname,
                }, {
                    "name": "environment",
                    "value": rollup.and_then(|r| r.environment.as_deref()).unwrap_or("unknown"),
                }, {
                    "name": "score",
                    "value": rollup.map(|r| r.score.to_string()).unwrap_or_default(),
                }]
            }));
        }
    }

    let doc = json!({
        "assessment-results": {
            "uuid": ar_uuid,
            "metadata": {
                "title": format!("{} Assessment Results", p.bundle.name),
                "last-modified": now,
                "version": "1.0",
                "oscal-version": "1.1.2",
                "props": [{
                    "name": "classification",
                    "value": "UNCLASSIFIED"
                }],
                "parties": [{
                    "uuid": uuid::Uuid::new_v4().to_string(),
                    "type": "tool",
                    "name": "Crystal Forge",
                }]
            },
            "import-ap": {
                "href": format!("#{}", ssp_uuid),
            },
            "local-definitions": {
                "components": components,
            },
            "results": [{
                "uuid": uuid::Uuid::new_v4().to_string(),
                "title": format!("{} v{} Assessment", p.bundle.name, p.bundle.version),
                "description": p.bundle.description.as_deref().unwrap_or(""),
                "start": now,
                "end": now,
                "prop": [{
                    "name": "overall-score",
                    "value": p.totals.overall_score.to_string(),
                }, {
                    "name": "framework",
                    "value": p.bundle.framework.clone(),
                }, {
                    "name": "compliant-hosts",
                    "value": format!("{} of {}", p.totals.fully_compliant_count, p.totals.system_count),
                }],
                "reviewed-controls": {
                    "description": format!("{} controls reviewed", p.totals.total_controls),
                    "control-selections": [{
                        "description": "All controls in bundle",
                        "include-all": {}
                    }]
                },
                "observations": observations,
                "findings": findings,
            }]
        }
    });

    serde_json::to_string_pretty(&doc).unwrap_or_default()
}

// ─── PDF (HTML print window) ──────────────────────────────────────────────────

/// Opens a styled print window.  The browser renders it to PDF via Ctrl-P.
/// Returns `Err` if `window.open()` is blocked.
/// Opens the HTML report as a Blob URL in a new tab and prints it.
/// Avoids `document.write` (requires feature-gated web-sys APIs) by instead
/// constructing a Blob URL and opening that directly.
pub fn open_print_window(p: &ExportPayload<'_>) -> Result<(), String> {
    use js_sys::Array;
    use web_sys::{BlobPropertyBag, Url};

    let html = build_print_html(p);

    // Build an HTML Blob and get an object URL for it
    let parts = Array::new();
    parts.push(&wasm_bindgen::JsValue::from_str(&html));
    let mut opts = BlobPropertyBag::new();
    opts.type_("text/html");
    let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &opts)
        .map_err(|_| "blob creation failed")?;
    let url = Url::create_object_url_with_blob(&blob).map_err(|_| "createObjectURL failed")?;

    // Open the blob URL in a new tab — the browser loads the HTML and the
    // inline `onload` handler triggers the print dialog.
    let window = web_sys::window().ok_or("no window")?;
    window
        .open_with_url_and_target_and_features(&url, "_blank", "")
        .map_err(|_| "window.open failed")?
        .ok_or("popup blocked — please allow popups for this site")?;

    // We cannot revoke immediately because the new tab still needs to load
    // the blob; revoke is deferred to GC / tab close by the browser.
    Ok(())
}

fn build_print_html(p: &ExportPayload<'_>) -> String {
    let now = js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default();
    let date = now.chars().take(10).collect::<String>();

    let mut body = String::new();

    // Cover section
    body.push_str(&format!(
        r#"<div class="cover">
  <h1>{}</h1>
  <p class="meta">Framework: {} {} · Layer: {} · Owner: {}</p>
  <p class="meta">Generated: {} · Overall score: {}% · Hosts: {} ({} fully compliant)</p>
  {}
</div>"#,
        esc(&p.bundle.name),
        esc(&p.bundle.framework),
        esc(&p.bundle.version),
        esc(&p.bundle.layer),
        esc(&p.bundle.owner),
        date,
        p.totals.overall_score,
        p.totals.system_count,
        p.totals.fully_compliant_count,
        p.bundle.description.as_deref().map(|d| format!("<p class=\"desc\">{}</p>", esc(d))).unwrap_or_default(),
    ));

    // Summary table
    body.push_str(
        r#"<h2>System summary</h2>
<table>
<thead><tr><th>Host</th><th>Environment</th><th>Score</th><th>Pass</th><th>Warn</th><th>Fail</th><th>Waiver</th></tr></thead>
<tbody>"#,
    );
    for s in p.scoped_systems() {
        let row_class = if s.fail > 0 { " class=\"fail-row\"" } else if s.warn > 0 { " class=\"warn-row\"" } else { "" };
        body.push_str(&format!(
            "<tr{}><td class=\"mono\">{}</td><td>{}</td><td><b>{}%</b></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            row_class,
            esc(&s.hostname),
            esc(s.environment.as_deref().unwrap_or("—")),
            s.score, s.pass, s.warn, s.fail, s.waiver,
        ));
    }
    body.push_str("</tbody></table>\n");

    // Per-system evidence detail
    let scoped_ev = p.scoped_evidence();
    for ev in &scoped_ev {
        body.push_str(&format!("<h2 class=\"host-heading\">Evidence: {}</h2>\n", esc(&ev.hostname)));
        body.push_str("<table><thead><tr><th>Control</th><th>Status</th><th>Severity</th><th>Mapping</th><th>Summary</th></tr></thead><tbody>\n");
        for ctrl in &ev.controls {
            if !p.include_waivers && matches!(ctrl.status, ComplianceControlStatus::Waiver) {
                continue;
            }
            let st = status_label(&ctrl.status);
            let cls = match ctrl.status {
                ComplianceControlStatus::Fail => " class=\"fail-row\"",
                ComplianceControlStatus::Warn => " class=\"warn-row\"",
                ComplianceControlStatus::Pass => " class=\"pass-row\"",
                ComplianceControlStatus::Waiver => "",
            };
            body.push_str(&format!(
                "<tr{}><td class=\"mono\">{}</td><td><b>{}</b></td><td>{}</td><td class=\"mono\">{}</td><td>{}</td></tr>\n",
                cls,
                esc(&ctrl.policy_name),
                st,
                esc(&ctrl.severity),
                esc(&ctrl.framework_mapping),
                esc(&ctrl.summary),
            ));
            if p.include_source {
                for item in &ctrl.evidence_items {
                    if let Some(art) = &item.artifact {
                        body.push_str(&format!(
                            "<tr class=\"evidence-row\"><td colspan=\"5\"><b>{}</b><pre>{}</pre></td></tr>\n",
                            esc(&art.title),
                            esc(&art.body),
                        ));
                    }
                }
            }
        }
        body.push_str("</tbody></table>\n");
    }

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8"/>
<title>{name} — Compliance Evidence Report</title>
<style>
  @page {{ size: A4 landscape; margin: 18mm; }}
  body {{ font-family: system-ui, sans-serif; font-size: 11px; color: #111; margin: 0; }}
  h1 {{ font-size: 20px; margin-bottom: 4px; }}
  h2 {{ font-size: 14px; margin: 18px 0 6px; border-bottom: 1px solid #ddd; padding-bottom: 4px; }}
  .host-heading {{ page-break-before: always; }}
  .cover {{ margin-bottom: 24px; }}
  .meta {{ color: #555; margin: 2px 0; }}
  .desc {{ margin-top: 8px; color: #333; }}
  table {{ border-collapse: collapse; width: 100%; margin-bottom: 12px; }}
  th {{ background: #1e1e2e; color: #fff; padding: 5px 8px; text-align: left; font-size: 10px; }}
  td {{ padding: 4px 8px; border-bottom: 1px solid #eee; vertical-align: top; }}
  .mono {{ font-family: monospace; font-size: 10px; }}
  .fail-row td {{ background: rgba(248,113,113,0.08); }}
  .warn-row td {{ background: rgba(251,191,36,0.08); }}
  .pass-row td {{ background: rgba(52,211,153,0.05); }}
  .evidence-row td {{ background: #f8f8f8; padding: 4px 8px; }}
  .evidence-row pre {{ margin: 4px 0 0; white-space: pre-wrap; word-break: break-all; font-size: 9px; color: #555; }}
  @media print {{
    .host-heading {{ page-break-before: always; }}
    th {{ background: #333 !important; -webkit-print-color-adjust: exact; print-color-adjust: exact; }}
    .fail-row td {{ background: rgba(248,113,113,0.15) !important; -webkit-print-color-adjust: exact; print-color-adjust: exact; }}
  }}
</style>
<script>window.onload = function() {{ window.print(); }}</script>
</head>
<body>
{body}
</body>
</html>"#,
        name = esc(&p.bundle.name),
        body = body,
    )
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn status_label(s: &ComplianceControlStatus) -> &'static str {
    match s {
        ComplianceControlStatus::Pass => "pass",
        ComplianceControlStatus::Warn => "warn",
        ComplianceControlStatus::Fail => "fail",
        ComplianceControlStatus::Waiver => "waiver",
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
