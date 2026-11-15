//! Serializes an already authorized, filtered CVE/package result for download.
//!
//! The caller owns authorization, scope intersection, filtering, ordering, and
//! the export row bound. This module does not query records. Every format
//! contains exactly the supplied rows, one row or observation per CVE/package
//! pair, in the supplied order.

use anyhow::Result;
use chrono::{DateTime, Utc};
use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use rust_xlsxwriter::Workbook;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::api::models::CveListItem;

const OSCAL_NS: &str = "http://csrc.nist.gov/ns/oscal/1.0";
const CF_NS: &str = "https://crystalforge.dev/ns/oscal/cve/1.0";

/// Column headers shared by the CSV and Excel encodings.
pub const HEADERS: [&str; 16] = [
    "CVE ID",
    "Severity",
    "CVSS Score",
    "Package",
    "Installed Version",
    "Fixed Version",
    "Affected Systems",
    "Currently Affected Systems",
    "Scheduled Deployment Targets",
    "Historical Inventory Systems",
    "Environments",
    "Fix Status",
    "Triage Status",
    "Age (days)",
    "First Seen",
    "Last Seen",
];

/// Selects one download encoding of the same filtered rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CveExportFormat {
    /// RFC 4180 CSV with [`HEADERS`].
    Csv,
    /// Office Open XML workbook with the CSV columns.
    Excel,
    /// OSCAL 1.1.2 assessment results, JSON encoding.
    OscalJson,
    /// OSCAL 1.1.2 assessment results, XML encoding.
    OscalXml,
}

impl CveExportFormat {
    /// Parses the `format` query value. Absence selects CSV, which keeps the
    /// original single-format endpoint behavior.
    ///
    /// # Errors
    ///
    /// Returns the rejected value when it names no supported format.
    pub fn parse(value: Option<&str>) -> std::result::Result<Self, String> {
        match value.unwrap_or("csv") {
            "csv" => Ok(Self::Csv),
            "excel" | "xlsx" => Ok(Self::Excel),
            "oscal-json" => Ok(Self::OscalJson),
            "oscal-xml" => Ok(Self::OscalXml),
            other => Err(other.to_owned()),
        }
    }

    /// Returns the filename extension and HTTP content type.
    pub fn file_type(self) -> (&'static str, &'static str) {
        match self {
            Self::Csv => ("csv", "text/csv"),
            Self::Excel => (
                "xlsx",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            ),
            Self::OscalJson => ("oscal.json", "application/json"),
            Self::OscalXml => ("oscal.xml", "application/xml"),
        }
    }
}

/// Describes the generated document, not any source record.
pub struct ExportContext<'a> {
    /// Sorted names of the requested, authorized environments. Empty means
    /// every environment visible to the caller.
    pub environments: &'a [String],
    /// Generation time of this document.
    pub generated_at: DateTime<Utc>,
}

/// Encodes `rows` in `format`.
///
/// # Errors
///
/// Returns an error when workbook or XML serialization fails. No partial
/// document is returned.
pub fn encode(
    format: CveExportFormat,
    rows: &[CveListItem],
    context: &ExportContext<'_>,
) -> Result<Vec<u8>> {
    match format {
        CveExportFormat::Csv => Ok(csv(rows).into_bytes()),
        CveExportFormat::Excel => xlsx(rows),
        CveExportFormat::OscalJson => Ok(serde_json::to_string_pretty(&oscal_json(
            &oscal_document(rows, context),
        ))?
        .into_bytes()),
        CveExportFormat::OscalXml => oscal_xml(&oscal_document(rows, context)),
    }
}

fn cells(cve: &CveListItem) -> [String; 16] {
    let date = |value: Option<DateTime<Utc>>| {
        value
            .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_default()
    };
    [
        cve.cve_id.clone(),
        cve.severity.clone(),
        cve.cvss_v3_score.map(|s| s.to_string()).unwrap_or_default(),
        cve.package_name.clone().unwrap_or_default(),
        cve.installed_version.clone().unwrap_or_default(),
        cve.fixed_version.clone().unwrap_or_default(),
        cve.affected_count.to_string(),
        cve.current_affected_count.to_string(),
        cve.scheduled_deployment_target_count.to_string(),
        cve.historical_inventory_count.to_string(),
        cve.affected_environments
            .as_deref()
            .unwrap_or_default()
            .join(";"),
        cve.fix_status.clone(),
        cve.triage_status.clone(),
        cve.age_days.to_string(),
        date(cve.first_seen),
        date(cve.last_seen),
    ]
}

/// Escapes one CSV field per RFC 4180: quote a value that contains a comma,
/// double quote, CR, or LF, and double each internal double quote.
pub fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn csv(rows: &[CveListItem]) -> String {
    let mut out = HEADERS.join(",");
    out.push('\n');
    for row in rows {
        let line: Vec<String> = cells(row).iter().map(|c| csv_field(c)).collect();
        out.push_str(&line.join(","));
        out.push('\n');
    }
    out
}

fn xlsx(rows: &[CveListItem]) -> Result<Vec<u8>> {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.set_name("CVEs")?;
    for (column, header) in HEADERS.iter().enumerate() {
        sheet.write_string(0, column as u16, *header)?;
    }
    for (index, row) in rows.iter().enumerate() {
        // SECURITY: `write_string` stores text cells. A value that starts
        // with `=` is not evaluated as a formula by spreadsheet consumers.
        for (column, value) in cells(row).iter().enumerate() {
            sheet.write_string(index as u32 + 1, column as u16, value.as_str())?;
        }
    }
    Ok(workbook.save_to_buffer()?)
}

fn title(context: &ExportContext<'_>) -> String {
    if context.environments.is_empty() {
        "Vulnerability assessment results".to_owned()
    } else {
        format!(
            "Vulnerability assessment results — {}",
            context.environments.join(", ")
        )
    }
}

/// Collapses whitespace so a value fits the single-line OSCAL `string` and
/// `markup-line` types (pattern `^\S(.*\S)?$`, no line breaks).
fn single_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Text of the back-matter resource that `import-ap` references.
///
/// OSCAL assessment results MUST reference an assessment plan. Crystal Forge
/// has no OSCAL assessment-plan document for vulnerability scans. The
/// reference resolves to this resource, which states that fact and the
/// method, instead of naming a plan that does not exist.
const NO_PLAN_TITLE: &str = "Crystal Forge vulnerability scan method";
const NO_PLAN_DESCRIPTION: &str = "Crystal Forge does not issue an OSCAL assessment plan for \
    vulnerability scans. Findings are package vulnerabilities that a scanner observed in deployed \
    or scheduled NixOS configurations, filtered by the export request. This resource is not an \
    assessment plan.";
/// Reviewed control: NIST SP 800-53 RA-5 (Vulnerability Monitoring and
/// Scanning). The OSCAL XML schema requires each control selection to name
/// `include-all` or explicit controls; `include-all` would claim that every
/// control was reviewed. A vulnerability scan is evidence for RA-5 only.
const REVIEWED_CONTROL_ID: &str = "ra-5";
const REVIEWED_CONTROLS_DESCRIPTION: &str = "Package vulnerability scanning evidence for RA-5. \
    No other control and no control implementation is assessed.";
const RESULT_DESCRIPTION: &str = "Filtered Crystal Forge CVE inventory.";

/// One OSCAL observation and finding pair for a CVE/package row.
struct OscalItem {
    observation_uuid: Uuid,
    finding_uuid: Uuid,
    title: String,
    description: String,
    props: Vec<(&'static str, String)>,
    cve_id: String,
}

/// The complete OSCAL document. Both encodings render this one value, so the
/// JSON and XML exports carry the same identities and content.
struct OscalDocument {
    uuid: Uuid,
    result_uuid: Uuid,
    plan_resource_uuid: Uuid,
    title: String,
    generated_at: String,
    version: String,
    items: Vec<OscalItem>,
}

// INVARIANT: Every finding target status is `not-satisfied`. A finding is an
// observed vulnerable package. Triage, including an accepted risk, does not
// remove the vulnerability, so it is reported as the `triage-status` property
// and never as `satisfied`. The target ID is the CVE ID; the objective it
// names is "no unremediated instance of this CVE in this package".
const TARGET_STATE: &str = "not-satisfied";

fn oscal_document(rows: &[CveListItem], context: &ExportContext<'_>) -> OscalDocument {
    let items = rows
        .iter()
        .map(|cve| {
            let package = cve.package_name.clone().unwrap_or_default();
            let mut props = vec![
                ("vulnerability-id", cve.cve_id.clone()),
                ("package", package.clone()),
                ("severity", cve.severity.to_lowercase()),
                (
                    "cvss-score",
                    cve.cvss_v3_score
                        .map(|s| format!("{s:.1}"))
                        .unwrap_or_default(),
                ),
                ("cvss-vector", cve.cvss_vector.clone().unwrap_or_default()),
                (
                    "installed-version",
                    cve.installed_version.clone().unwrap_or_default(),
                ),
                ("fixed-in", cve.fixed_version.clone().unwrap_or_default()),
                ("fix-status", cve.fix_status.clone()),
                ("triage-status", cve.triage_status.clone()),
                ("affected-systems", cve.affected_count.to_string()),
                (
                    "environments",
                    cve.affected_environments
                        .as_deref()
                        .unwrap_or_default()
                        .join(";"),
                ),
            ];
            if cve.exploited {
                props.push(("known-exploited", "true".to_owned()));
            }
            let props = props
                .into_iter()
                .map(|(name, value)| (name, single_line(&value)))
                .filter(|(_, value)| !value.is_empty())
                .collect();
            let description = if cve.title.trim().is_empty() {
                cve.cve_id.clone()
            } else {
                cve.title.trim().to_owned()
            };
            OscalItem {
                observation_uuid: Uuid::new_v4(),
                finding_uuid: Uuid::new_v4(),
                title: single_line(&if package.is_empty() {
                    cve.cve_id.clone()
                } else {
                    format!("{} in {package}", cve.cve_id)
                }),
                description,
                props,
                cve_id: cve.cve_id.clone(),
            }
        })
        .collect();
    OscalDocument {
        uuid: Uuid::new_v4(),
        result_uuid: Uuid::new_v4(),
        plan_resource_uuid: Uuid::new_v4(),
        title: single_line(&title(context)),
        generated_at: context.generated_at.to_rfc3339(),
        version: context.generated_at.format("%Y-%m-%d").to_string(),
        items,
    }
}

fn oscal_json(doc: &OscalDocument) -> Value {
    let props = |props: &[(&str, String)]| -> Vec<Value> {
        props
            .iter()
            .map(|(name, value)| json!({"name": name, "ns": CF_NS, "value": value}))
            .collect()
    };
    let mut result = json!({
        "uuid": doc.result_uuid,
        "title": "Fleet vulnerability scan",
        "description": RESULT_DESCRIPTION,
        "start": doc.generated_at,
        "reviewed-controls": {
            "control-selections": [{
                "description": REVIEWED_CONTROLS_DESCRIPTION,
                "include-controls": [{"control-id": REVIEWED_CONTROL_ID}],
            }],
        },
    });
    // OSCAL arrays require at least one member; omit them for an empty result.
    if !doc.items.is_empty() {
        result["observations"] = doc
            .items
            .iter()
            .map(|item| {
                json!({
                    "uuid": item.observation_uuid,
                    "title": item.title,
                    "description": item.description,
                    "props": props(&item.props),
                    "methods": ["TEST"],
                    "types": ["finding"],
                    "collected": doc.generated_at,
                })
            })
            .collect();
        result["findings"] = doc
            .items
            .iter()
            .map(|item| {
                json!({
                    "uuid": item.finding_uuid,
                    "title": item.title,
                    "description": item.description,
                    "target": {
                        "type": "objective-id",
                        "target-id": item.cve_id,
                        "status": {"state": TARGET_STATE},
                    },
                    "related-observations": [{"observation-uuid": item.observation_uuid}],
                })
            })
            .collect();
    }
    json!({
        "assessment-results": {
            "uuid": doc.uuid,
            "metadata": {
                "title": doc.title,
                "last-modified": doc.generated_at,
                "version": doc.version,
                "oscal-version": "1.1.2",
            },
            "import-ap": {"href": format!("#{}", doc.plan_resource_uuid)},
            "results": [result],
            "back-matter": {
                "resources": [{
                    "uuid": doc.plan_resource_uuid,
                    "title": NO_PLAN_TITLE,
                    "description": NO_PLAN_DESCRIPTION,
                }],
            },
        }
    })
}

fn oscal_xml(doc: &OscalDocument) -> Result<Vec<u8>> {
    let mut xml = Writer::new(Vec::new());
    xml.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;
    let document_uuid = doc.uuid.to_string();
    start(
        &mut xml,
        "assessment-results",
        &[("xmlns", OSCAL_NS), ("uuid", &document_uuid)],
    )?;
    start(&mut xml, "metadata", &[])?;
    text(&mut xml, "title", &doc.title)?;
    text(&mut xml, "last-modified", &doc.generated_at)?;
    text(&mut xml, "version", &doc.version)?;
    text(&mut xml, "oscal-version", "1.1.2")?;
    end(&mut xml, "metadata")?;
    empty(
        &mut xml,
        "import-ap",
        &[("href", &format!("#{}", doc.plan_resource_uuid))],
    )?;
    // INVARIANT: OSCAL XML element order is schema-defined (XSD sequences).
    start(
        &mut xml,
        "result",
        &[("uuid", &doc.result_uuid.to_string())],
    )?;
    text(&mut xml, "title", "Fleet vulnerability scan")?;
    paragraph(&mut xml, RESULT_DESCRIPTION)?;
    text(&mut xml, "start", &doc.generated_at)?;
    start(&mut xml, "reviewed-controls", &[])?;
    start(&mut xml, "control-selection", &[])?;
    paragraph(&mut xml, REVIEWED_CONTROLS_DESCRIPTION)?;
    empty(
        &mut xml,
        "include-control",
        &[("control-id", REVIEWED_CONTROL_ID)],
    )?;
    end(&mut xml, "control-selection")?;
    end(&mut xml, "reviewed-controls")?;
    for item in &doc.items {
        start(
            &mut xml,
            "observation",
            &[("uuid", &item.observation_uuid.to_string())],
        )?;
        text(&mut xml, "title", &item.title)?;
        paragraph(&mut xml, &item.description)?;
        for (name, value) in &item.props {
            empty(
                &mut xml,
                "prop",
                &[("name", name), ("ns", CF_NS), ("value", value)],
            )?;
        }
        text(&mut xml, "method", "TEST")?;
        text(&mut xml, "type", "finding")?;
        text(&mut xml, "collected", &doc.generated_at)?;
        end(&mut xml, "observation")?;
    }
    for item in &doc.items {
        start(
            &mut xml,
            "finding",
            &[("uuid", &item.finding_uuid.to_string())],
        )?;
        text(&mut xml, "title", &item.title)?;
        paragraph(&mut xml, &item.description)?;
        start(
            &mut xml,
            "target",
            &[("type", "objective-id"), ("target-id", &item.cve_id)],
        )?;
        empty(&mut xml, "status", &[("state", TARGET_STATE)])?;
        end(&mut xml, "target")?;
        empty(
            &mut xml,
            "related-observation",
            &[("observation-uuid", &item.observation_uuid.to_string())],
        )?;
        end(&mut xml, "finding")?;
    }
    end(&mut xml, "result")?;
    start(&mut xml, "back-matter", &[])?;
    start(
        &mut xml,
        "resource",
        &[("uuid", &doc.plan_resource_uuid.to_string())],
    )?;
    text(&mut xml, "title", NO_PLAN_TITLE)?;
    paragraph(&mut xml, NO_PLAN_DESCRIPTION)?;
    end(&mut xml, "resource")?;
    end(&mut xml, "back-matter")?;
    end(&mut xml, "assessment-results")?;
    Ok(xml.into_inner())
}

fn element<'a>(name: &'a str, attributes: &[(&str, &str)]) -> BytesStart<'a> {
    let mut element = BytesStart::new(name);
    for attribute in attributes {
        element.push_attribute(*attribute);
    }
    element
}

fn start(writer: &mut Writer<Vec<u8>>, name: &str, attributes: &[(&str, &str)]) -> Result<()> {
    writer.write_event(Event::Start(element(name, attributes)))?;
    Ok(())
}

fn empty(writer: &mut Writer<Vec<u8>>, name: &str, attributes: &[(&str, &str)]) -> Result<()> {
    writer.write_event(Event::Empty(element(name, attributes)))?;
    Ok(())
}

fn end(writer: &mut Writer<Vec<u8>>, name: &str) -> Result<()> {
    writer.write_event(Event::End(BytesEnd::new(name)))?;
    Ok(())
}

fn text(writer: &mut Writer<Vec<u8>>, name: &str, value: &str) -> Result<()> {
    start(writer, name, &[])?;
    writer.write_event(Event::Text(BytesText::new(value)))?;
    end(writer, name)
}

fn paragraph(writer: &mut Writer<Vec<u8>>, value: &str) -> Result<()> {
    start(writer, "description", &[])?;
    text(writer, "p", value)?;
    end(writer, "description")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(cve: &str, package: &str, triage: &str) -> CveListItem {
        CveListItem {
            cve_id: cve.into(),
            cvss_v3_score: Some(9.8),
            severity: "CRITICAL".into(),
            title: "Heap overflow, \"remote\"".into(),
            cvss_vector: None,
            published_date: None,
            exploited: true,
            package_name: Some(package.into()),
            installed_version: Some("1.0".into()),
            fixed_version: Some("1.1".into()),
            fix_status: "fix_available".into(),
            affected_count: 2,
            exact_affected_count: 2,
            legacy_affected_count: 0,
            current_affected_count: 2,
            scheduled_deployment_target_count: 0,
            historical_inventory_count: 0,
            affected_environments: Some(vec!["dev".into(), "prod".into()]),
            first_seen: None,
            last_seen: None,
            age_days: 3,
            triage_status: triage.into(),
        }
    }

    fn context(environments: &[String]) -> ExportContext<'_> {
        ExportContext {
            environments,
            generated_at: Utc::now(),
        }
    }

    #[test]
    fn format_parse_defaults_to_csv_and_rejects_unknown() {
        assert_eq!(CveExportFormat::parse(None), Ok(CveExportFormat::Csv));
        assert_eq!(
            CveExportFormat::parse(Some("oscal-xml")),
            Ok(CveExportFormat::OscalXml)
        );
        assert_eq!(CveExportFormat::parse(Some("pdf")), Err("pdf".to_string()));
    }

    #[test]
    fn csv_has_header_and_one_line_per_row() {
        let rows = [
            row("CVE-2026-0001", "openssl", "outstanding"),
            row("CVE-2026-0002", "curl", "accepted"),
        ];
        let out =
            String::from_utf8(encode(CveExportFormat::Csv, &rows, &context(&[])).unwrap()).unwrap();
        assert_eq!(out.lines().count(), 3);
        assert_eq!(
            HEADERS.len(),
            out.lines().next().unwrap().split(',').count()
        );
        assert!(out.contains("CVE-2026-0002,CRITICAL,9.8,curl,"));
        assert!(out.contains("dev;prod"));
    }

    #[test]
    fn csv_field_escapes_rfc4180_specials() {
        assert_eq!(csv_field("openssl"), "openssl");
        assert_eq!(csv_field("foo,bar"), "\"foo,bar\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_field("a\nb"), "\"a\nb\"");
    }

    #[test]
    fn excel_is_a_zip_workbook() {
        let rows = [row("CVE-2026-0001", "openssl", "outstanding")];
        let bytes = encode(CveExportFormat::Excel, &rows, &context(&[])).unwrap();
        assert!(bytes.starts_with(b"PK"));
    }

    #[test]
    fn oscal_json_has_one_observation_and_finding_per_row() {
        let rows = [
            row("CVE-2026-0001", "openssl", "outstanding"),
            row("CVE-2026-0002", "curl", "accepted"),
        ];
        let envs = vec!["dev".to_string(), "prod".to_string()];
        let bytes = encode(CveExportFormat::OscalJson, &rows, &context(&envs)).unwrap();
        let doc: Value = serde_json::from_slice(&bytes).unwrap();
        let ar = &doc["assessment-results"];
        assert_eq!(ar["metadata"]["oscal-version"], "1.1.2");
        assert!(
            ar["metadata"]["title"]
                .as_str()
                .unwrap()
                .ends_with("dev, prod")
        );
        let result = &ar["results"][0];
        assert_eq!(result["observations"].as_array().unwrap().len(), 2);
        let findings = result["findings"].as_array().unwrap();
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0]["target"]["status"]["state"], "not-satisfied");
        // Accepted risk does not satisfy the objective; triage is a property.
        assert_eq!(findings[1]["target"]["status"]["state"], "not-satisfied");
        assert_eq!(
            findings[1]["related-observations"][0]["observation-uuid"],
            result["observations"][1]["uuid"]
        );
    }

    #[test]
    fn oscal_json_omits_empty_arrays() {
        let bytes = encode(CveExportFormat::OscalJson, &[], &context(&[])).unwrap();
        let doc: Value = serde_json::from_slice(&bytes).unwrap();
        let result = &doc["assessment-results"]["results"][0];
        assert!(result.get("observations").is_none());
        assert!(result.get("findings").is_none());
    }

    #[test]
    fn oscal_xml_uses_official_namespace_and_escapes_text() {
        let rows = [row("CVE-2026-0001", "a<b", "outstanding")];
        let out =
            String::from_utf8(encode(CveExportFormat::OscalXml, &rows, &context(&[])).unwrap())
                .unwrap();
        assert!(out.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(out.contains("<assessment-results xmlns=\"http://csrc.nist.gov/ns/oscal/1.0\""));
        assert_eq!(out.matches("<observation ").count(), 1);
        assert_eq!(out.matches("<finding ").count(), 1);
        assert!(out.contains("a&lt;b"));
        assert!(!out.contains("a<b"));
    }

    #[test]
    fn oscal_import_ap_resolves_to_a_back_matter_resource() {
        let rows = [row("CVE-2026-0001", "openssl", "outstanding")];
        let doc = oscal_document(&rows, &context(&[]));
        let json = oscal_json(&doc);
        let ar = &json["assessment-results"];
        let href = ar["import-ap"]["href"].as_str().unwrap();
        let resource = &ar["back-matter"]["resources"][0];
        assert_eq!(href, format!("#{}", resource["uuid"].as_str().unwrap()));
        assert_eq!(resource["title"], NO_PLAN_TITLE);
        let xml = String::from_utf8(oscal_xml(&doc).unwrap()).unwrap();
        assert!(xml.contains(&format!(
            "<import-ap href=\"#{}\"/>",
            doc.plan_resource_uuid
        )));
        assert!(xml.contains(&format!("<resource uuid=\"{}\">", doc.plan_resource_uuid)));
        assert!(
            ar["results"][0]["reviewed-controls"]["control-selections"][0]["description"]
                .is_string()
        );
    }

    #[test]
    fn oscal_json_and_xml_encode_the_same_document() {
        let rows = [
            row("CVE-2026-0001", "openssl", "outstanding"),
            row("CVE-2026-0002", "curl", "accepted"),
        ];
        let doc = oscal_document(&rows, &context(&[]));
        let json = serde_json::to_string(&oscal_json(&doc)).unwrap();
        let xml = String::from_utf8(oscal_xml(&doc).unwrap()).unwrap();
        for id in [doc.uuid, doc.result_uuid, doc.plan_resource_uuid]
            .into_iter()
            .chain(
                doc.items
                    .iter()
                    .flat_map(|i| [i.observation_uuid, i.finding_uuid]),
            )
        {
            let id = id.to_string();
            assert!(json.contains(&id) && xml.contains(&id), "{id} in both");
        }
        for item in &doc.items {
            for (name, value) in &item.props {
                assert!(json.contains(&format!("\"value\":\"{value}\"")));
                assert!(xml.contains(&format!("name=\"{name}\"")));
            }
        }
        assert_eq!(
            json.matches("\"target-id\"").count(),
            xml.matches("target-id=").count()
        );
    }

    #[test]
    fn oscal_titles_and_property_values_are_single_line() {
        let mut multi = row("CVE-2026-0003", " zlib\n", "outstanding");
        multi.title = "line one\nline two".into();
        multi.cvss_vector = Some("  AV:N/AC:L \n".into());
        let doc = oscal_document(&[multi], &context(&[]));
        let item = &doc.items[0];
        assert_eq!(item.title, "CVE-2026-0003 in zlib");
        assert!(
            item.props
                .iter()
                .all(|(_, v)| !v.contains('\n') && v.trim() == v)
        );
        assert!(item.props.contains(&("cvss-vector", "AV:N/AC:L".into())));
        // Multi-line text is allowed only in markup-multiline descriptions.
        assert_eq!(item.description, "line one\nline two");
    }

    /// Writes empty, single-row, and multi-row OSCAL samples to
    /// `CF_OSCAL_SAMPLE_DIR` for validation against the official NIST
    /// schemas. Does nothing when the variable is unset. See the CVE export
    /// section of `docs/knowledge/api/fleet-cve-triage-api.md`.
    #[test]
    fn write_oscal_samples_for_schema_validation() {
        let Ok(dir) = std::env::var("CF_OSCAL_SAMPLE_DIR") else {
            return;
        };
        let mut multiline = row("CVE-2026-0004", "glibc", "scheduled");
        multiline.title = "First line.\nSecond line with <tag> & \"quotes\".".into();
        multiline.exploited = false;
        multiline.fixed_version = None;
        let many = [
            row("CVE-2026-0001", "openssl", "outstanding"),
            row("CVE-2026-0002", "curl", "accepted"),
            row("CVE-2026-0003", "zlib", "inventory_only"),
            multiline,
        ];
        let envs = vec!["dev".to_string(), "prod".to_string()];
        let cases: [(&str, &[CveListItem]); 3] =
            [("empty", &[]), ("single", &many[..1]), ("multi", &many)];
        for (name, rows) in cases {
            for (format, ext) in [
                (CveExportFormat::OscalJson, "json"),
                (CveExportFormat::OscalXml, "xml"),
            ] {
                let bytes = encode(format, rows, &context(&envs)).unwrap();
                std::fs::write(format!("{dir}/{name}.{ext}"), bytes).unwrap();
            }
        }
    }
}
