//! Writes standalone OSCAL 1.1.2 POA&M documents from an already authorized snapshot.
//!
//! This module does not fetch or authorize records. Callers must supply the entire
//! scoped result and stable document metadata. It does not infer controls,
//! observations, or approvals from the presence of a finding or a disposition.

use anyhow::{Result, ensure};
use chrono::{DateTime, NaiveDate, Utc};
use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use serde_json::{Value, json};
use uuid::Uuid;

const NS: &str = "http://csrc.nist.gov/ns/oscal/1.0";
const CF_NS: &str = "https://crystalforge.dev/ns/oscal/poam/1.0";

/// Identifies a generated document over an authorized, complete source snapshot.
pub struct Document<'a> {
    /// UUID generated for this export document, distinct from source UUIDs.
    pub uuid: Uuid,
    /// Human-readable document title.
    pub title: &'a str,
    /// Version of this generated export document.
    pub version: &'a str,
    /// Time this document was generated, not a source change timestamp.
    pub last_modified: DateTime<Utc>,
    /// All authorized POA&M and acceptance entries in the requested scope.
    pub entries: &'a [Entry<'a>],
}

/// Identifies the distinct source lifecycle represented by an OSCAL POA&M item.
pub enum Source<'a> {
    /// An actual remediation plan with its source status.
    Plan {
        /// Persisted plan status.
        status: &'a str,
        /// Source creation time.
        created_at: DateTime<Utc>,
        /// Source mutation time.
        updated_at: DateTime<Utc>,
        /// Persisted owner display, when recorded.
        owner: Option<&'a str>,
        /// Typed assigned user, when recorded.
        assignee_user_id: Option<Uuid>,
        /// Typed assigned group, when recorded.
        assignee_group_name: Option<&'a str>,
    },
    /// A source decision, including pending and retired decisions.
    Acceptance {
        /// Operator-facing RA renewal chain, separate from typed source ID.
        human_id: Option<&'a str>,
        /// Source table family; a pending waiver is not an approved risk.
        source_kind: &'a str,
        /// Persisted decision rationale; it is not a measured impact.
        rationale: &'a str,
        /// Persisted approving user UUID, when present.
        accepted_by: Option<Uuid>,
        /// Persisted approval timestamp, when present.
        accepted_at: Option<DateTime<Utc>>,
        /// Source decision creation or acceptance time.
        recorded_at: DateTime<Utc>,
        /// Source review date, if recorded.
        review_date: Option<NaiveDate>,
        /// Source review deadline, if recorded.
        review_due_at: Option<NaiveDate>,
        /// Source authorization expiry, if recorded.
        expires_at: Option<DateTime<Utc>>,
        /// Source retirement time and actor, if recorded.
        retired_at: Option<DateTime<Utc>>,
        /// Persisted retirement actor, if recorded.
        retired_by: Option<Uuid>,
        /// Persisted reason for retirement, if recorded.
        retirement_reason: Option<&'a str>,
        /// Persisted replacement plan UUID, if recorded.
        replacement_poam_id: Option<Uuid>,
        /// Source lifecycle status, not an OSCAL risk status.
        status: &'a str,
    },
}

/// One source record in the caller's authorized scope.
pub struct Entry<'a> {
    /// Stable source UUID, reused as the OSCAL item UUID.
    pub uuid: Uuid,
    /// Source-specific ID, such as a plan number or acceptance decision ID.
    pub source_id: &'a str,
    /// Source title.
    pub title: &'a str,
    /// Source description, possibly empty when no plan narrative was recorded.
    pub description: &'a str,
    /// All source-recorded scope identifiers, or empty for no recorded scope.
    pub scopes: &'a [String],
    /// All linked and scheduled canonical CVE IDs from the authorized snapshot.
    pub cve_ids: &'a [String],
    /// Exact scheduled environment tuples, which do not imply a finding.
    pub scheduled_cve_tuples: &'a [ScheduledCveTuple],
    /// Source-linked CVE finding identities and optional baseline scans.
    pub cve_finding_links: &'a [CveFindingLink],
    /// Persisted policy finding UUIDs linked to this source item.
    pub policy_finding_ids: &'a [Uuid],
    /// Date-only source target or review deadline; never a midnight timestamp.
    pub target_date: Option<NaiveDate>,
    /// Recorded technical evidence description, if any.
    pub technical_evidence: Option<&'a str>,
    /// The actual source lifecycle.
    pub source: Source<'a>,
}

/// Identifies a scheduled CVE decision without asserting a current observation.
#[derive(Clone)]
pub struct ScheduledCveTuple {
    /// Canonical CVE identity.
    pub canonical_cve_id: String,
    /// Canonical package identity.
    pub canonical_package_name: String,
}

/// Identifies one persisted CVE finding link and its optional baseline scan.
#[derive(Clone)]
pub struct CveFindingLink {
    /// Stable linked CVE finding UUID.
    pub finding_id: Uuid,
    /// Canonical CVE identity.
    pub canonical_cve_id: String,
    /// Canonical package identity.
    pub canonical_package_name: String,
    /// Link-time scan UUID when the baseline recorded one.
    pub baseline_scan_id: Option<Uuid>,
}

/// Returns the same authorized records in both OSCAL encodings.
#[derive(Debug)]
pub struct Encodings {
    /// OSCAL 1.1.2 POA&M JSON document.
    pub json: String,
    /// OSCAL 1.1.2 POA&M XML document in the official namespace.
    pub xml: String,
}

/// Writes a POA&M item for every source record without inventing risk impact.
///
/// Source-specific facts use namespaced OSCAL properties. No finding or
/// observation is emitted without assessment evidence and a real target.
///
/// # Errors
/// Returns an error for missing document metadata, empty or duplicate entries,
/// incomplete source records, or an inconsistent persisted approval pair.
/// Serialization errors are returned without a partial document.
///
/// # Examples
/// ```ignore
/// let documents = write_poam(&authorized_snapshot)?;
/// // Deliver both documents to the same authorized caller.
/// ```
pub fn write_poam(document: &Document<'_>) -> Result<Encodings> {
    ensure!(
        !document.title.trim().is_empty(),
        "POA&M document title is required"
    );
    ensure!(
        !document.version.trim().is_empty(),
        "POA&M document version is required"
    );
    // OSCAL 1.1.2 requires `poam-items` with at least one item. An empty
    // authorized selection is handled as HTTP 204 before invoking this writer.
    ensure!(!document.entries.is_empty(), "POA&M export has no items");
    let seen: std::collections::HashSet<_> =
        document.entries.iter().map(|entry| entry.uuid).collect();
    ensure!(
        seen.len() == document.entries.len(),
        "duplicate POA&M item UUID"
    );
    let mut items = Vec::with_capacity(document.entries.len());
    let mut xml = Writer::new(Vec::new());
    xml.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;
    let mut root = BytesStart::new("plan-of-action-and-milestones");
    let document_uuid = document.uuid.to_string();
    root.push_attribute(("xmlns", NS));
    root.push_attribute(("uuid", document_uuid.as_str()));
    xml.write_event(Event::Start(root))?;
    start(&mut xml, "metadata")?;
    text(&mut xml, "title", document.title)?;
    text(
        &mut xml,
        "last-modified",
        &document.last_modified.to_rfc3339(),
    )?;
    text(&mut xml, "version", document.version)?;
    text(&mut xml, "oscal-version", "1.1.2")?;
    end(&mut xml, "metadata")?;

    for entry in document.entries {
        for (label, value) in [("source ID", entry.source_id), ("title", entry.title)] {
            ensure!(
                !value.trim().is_empty(),
                "POA&M item {} has no {label}",
                entry.uuid
            );
        }
        for cve in entry.cve_ids {
            ensure!(
                !cve.trim().is_empty(),
                "POA&M item {} has an empty CVE ID",
                entry.uuid
            );
        }
        let (kind, status) = match &entry.source {
            Source::Plan { status, .. } => ("plan", *status),
            Source::Acceptance {
                source_kind,
                status,
                ..
            } => (*source_kind, *status),
        };
        ensure!(
            !status.trim().is_empty(),
            "POA&M item {} has no status",
            entry.uuid
        );
        let mut props = vec![
            ("source-kind", kind.to_owned()),
            ("source-id", entry.source_id.to_owned()),
            ("source-status", status.to_owned()),
        ];
        if entry.scopes.is_empty() {
            props.push(("scope-state", "not-recorded".to_owned()));
        }
        for scope in entry.scopes {
            ensure!(
                !scope.trim().is_empty(),
                "POA&M item {} has empty scope",
                entry.uuid
            );
            props.push(("scope", scope.clone()));
        }
        for cve in entry.cve_ids {
            props.push(("cve-id", cve.clone()));
        }
        // Each namespaced property holds an atomic source tuple; a scan-less
        // baseline never gains an inferred scan or observation.
        for tuple in entry.scheduled_cve_tuples {
            ensure!(
                !tuple.canonical_cve_id.trim().is_empty()
                    && !tuple.canonical_package_name.trim().is_empty(),
                "empty scheduled CVE tuple"
            );
            props.push((
                "scheduled-environment-cve-tuple",
                json!({
                    "canonical_cve_id": tuple.canonical_cve_id,
                    "canonical_package_name": tuple.canonical_package_name,
                })
                .to_string(),
            ));
        }
        for link in entry.cve_finding_links {
            ensure!(
                !link.canonical_cve_id.trim().is_empty()
                    && !link.canonical_package_name.trim().is_empty(),
                "empty CVE finding link tuple"
            );
            let mut tuple = json!({
                "finding_id": link.finding_id,
                "canonical_cve_id": link.canonical_cve_id,
                "canonical_package_name": link.canonical_package_name,
            });
            if let Some(scan) = link.baseline_scan_id {
                tuple["baseline_scan_id"] = json!(scan);
            }
            props.push(("cve-finding-link", tuple.to_string()));
        }
        for finding in entry.policy_finding_ids {
            props.push(("policy-finding-id", finding.to_string()));
        }
        if let Some(date) = entry.target_date {
            props.push(("source-target-date", date.to_string()));
        }
        if let Some(evidence) = entry.technical_evidence {
            ensure!(
                !evidence.trim().is_empty(),
                "POA&M item {} has empty evidence",
                entry.uuid
            );
            props.push(("technical-evidence", evidence.to_owned()));
        }
        if let Source::Plan {
            created_at,
            updated_at,
            owner,
            assignee_user_id,
            assignee_group_name,
            ..
        } = &entry.source
        {
            ensure!(
                assignee_user_id.is_none() || assignee_group_name.is_none(),
                "plan {} has conflicting typed assignees",
                entry.uuid
            );
            props.push(("plan-created-at", created_at.to_rfc3339()));
            props.push(("plan-updated-at", updated_at.to_rfc3339()));
            if let Some(owner) = owner {
                ensure!(
                    !owner.trim().is_empty(),
                    "plan {} has empty owner",
                    entry.uuid
                );
                props.push(("plan-owner", (*owner).to_owned()));
            }
            if let Some(user) = assignee_user_id {
                props.push(("plan-assignee-user-id", user.to_string()));
            }
            if let Some(group) = assignee_group_name {
                ensure!(
                    !group.trim().is_empty(),
                    "plan {} has empty group",
                    entry.uuid
                );
                props.push(("plan-assignee-group-name", (*group).to_owned()));
            }
        }
        if let Source::Acceptance {
            human_id,
            rationale,
            accepted_by,
            accepted_at,
            recorded_at,
            review_date,
            review_due_at,
            expires_at,
            retired_at,
            retired_by,
            retirement_reason,
            replacement_poam_id,
            ..
        } = &entry.source
        {
            if let Some(human_id) = human_id {
                ensure!(
                    human_id.starts_with("RA-")
                        && human_id[3..].len() >= 4
                        && human_id[3..].bytes().all(|byte| byte.is_ascii_digit()),
                    "acceptance {} has invalid RA ID",
                    entry.uuid
                );
                props.push(("risk-acceptance-id", (*human_id).to_owned()));
            }
            ensure!(
                !rationale.trim().is_empty(),
                "acceptance {} needs a decision rationale",
                entry.uuid
            );
            ensure!(
                accepted_by.is_some() == accepted_at.is_some(),
                "acceptance {} has incomplete approval context",
                entry.uuid
            );
            props.push(("decision-rationale", (*rationale).to_owned()));
            props.push(("decision-recorded-at", recorded_at.to_rfc3339()));
            if let Some(date) = review_date {
                props.push(("decision-review-date", date.to_string()));
            }
            if let Some(date) = review_due_at {
                props.push(("decision-review-due-date", date.to_string()));
            }
            if let Some(time) = expires_at {
                props.push(("decision-expires-at", time.to_rfc3339()));
            }
            if let Some(time) = retired_at {
                props.push(("decision-retired-at", time.to_rfc3339()));
            }
            if let Some(actor) = retired_by {
                props.push(("decision-retired-by-user-id", actor.to_string()));
            }
            if let Some(reason) = retirement_reason {
                ensure!(
                    !reason.trim().is_empty(),
                    "acceptance {} has empty retirement reason",
                    entry.uuid
                );
                props.push(("decision-retirement-reason", (*reason).to_owned()));
            }
            if let Some(plan) = replacement_poam_id {
                props.push(("decision-replacement-poam-id", plan.to_string()));
            }
            if let (Some(user), Some(time)) = (accepted_by, accepted_at) {
                props.push(("accepted-by-user-id", user.to_string()));
                props.push(("accepted-at", time.to_rfc3339()));
            }
        }
        let item = json!({
            "uuid": entry.uuid,
            "title": entry.title,
            "description": entry.description,
            "props": props.iter().map(|(name, value)| json!({"ns": CF_NS, "name": name, "value": value})).collect::<Vec<Value>>(),
        });
        items.push(item);
    }

    for (entry, item) in document.entries.iter().zip(&items) {
        let id = entry.uuid.to_string();
        start_uuid(&mut xml, "poam-item", &id)?;
        text(&mut xml, "title", entry.title)?;
        paragraph(&mut xml, "description", entry.description)?;
        for prop in item["props"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("invalid POA&M properties"))?
        {
            let mut element = BytesStart::new("prop");
            element.push_attribute(("ns", CF_NS));
            element.push_attribute((
                "name",
                prop["name"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("invalid property name"))?,
            ));
            element.push_attribute((
                "value",
                prop["value"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("invalid property value"))?,
            ));
            xml.write_event(Event::Empty(element))?;
        }
        end(&mut xml, "poam-item")?;
    }
    end(&mut xml, "plan-of-action-and-milestones")?;

    let poam = json!({
        "uuid": document.uuid,
        "metadata": {
            "title": document.title,
            "last-modified": document.last_modified.to_rfc3339(),
            "version": document.version,
            "oscal-version": "1.1.2",
        },
        "poam-items": items,
    });
    Ok(Encodings {
        json: serde_json::to_string_pretty(&json!({"plan-of-action-and-milestones": poam}))?,
        xml: String::from_utf8(xml.into_inner())?,
    })
}

fn start(writer: &mut Writer<Vec<u8>>, name: &str) -> Result<()> {
    writer.write_event(Event::Start(BytesStart::new(name)))?;
    Ok(())
}

fn start_uuid(writer: &mut Writer<Vec<u8>>, name: &str, uuid: &str) -> Result<()> {
    let mut element = BytesStart::new(name);
    element.push_attribute(("uuid", uuid));
    writer.write_event(Event::Start(element))?;
    Ok(())
}

fn text(writer: &mut Writer<Vec<u8>>, name: &str, value: &str) -> Result<()> {
    start(writer, name)?;
    writer.write_event(Event::Text(BytesText::new(value)))?;
    end(writer, name)
}

fn paragraph(writer: &mut Writer<Vec<u8>>, name: &str, value: &str) -> Result<()> {
    start(writer, name)?;
    text(writer, "p", value)?;
    end(writer, name)
}

fn end(writer: &mut Writer<Vec<u8>>, name: &str) -> Result<()> {
    writer.write_event(Event::End(BytesEnd::new(name)))?;
    Ok(())
}
