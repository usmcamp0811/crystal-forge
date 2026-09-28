//! Serializes an already authorized, complete register snapshot as CSV and XLSX.
//!
//! The caller owns authorization, whole-scope pagination, filtering, and snapshot
//! consistency. This writer does not query records, authorize access, or infer
//! evidence. It rejects oversized input rather than exporting a partial register.

use anyhow::{Result, ensure};
use chrono::{DateTime, NaiveDate, Utc};
use rust_xlsxwriter::Workbook;
use std::collections::HashSet;
use uuid::Uuid;

/// Maximum number of source records in one export.
pub const MAX_ITEMS: usize = 10_000;
/// Maximum number of data rows after expanding source evidence links.
pub const MAX_ROWS: usize = 50_000;

const MAX_CELL_CHARS: usize = 32_767;
const HEADERS: [&str; 16] = [
    "Source type",
    "Source UUID",
    "Source ID",
    "Title",
    "Status",
    "Scope type",
    "Scope UUID",
    "Scope name",
    "CVE ID",
    "Target date",
    "Review deadline",
    "Authorization expiry",
    "Finding UUID",
    "Scan UUID",
    "Evidence",
    "Description",
];

/// Distinguishes a system from an environment without inferring scope from its name.
pub enum Scope<'a> {
    /// An exact system identity and its display name.
    System { id: Uuid, name: &'a str },
    /// An exact environment identity and its display name.
    Environment { id: Uuid, name: &'a str },
    /// No scope identity is recorded by the source; the register context is not scope.
    Unspecified,
    /// Every distinct recorded identity. The three scope columns contain aligned JSON arrays,
    /// sorted by scope type and UUID, so evidence expansion never multiplies source rows.
    Multiple { scopes: &'a [Scope<'a>] },
}

/// Identifies the actual source lifecycle and keeps review distinct from expiry.
pub enum Source {
    /// A remediation plan with its scheduled target date.
    Plan {
        /// Scheduled remediation target, not an acceptance review or expiry.
        target_date: Option<NaiveDate>,
    },
    /// A policy or CVE acceptance decision with separate validity dates.
    Acceptance {
        /// Whether the decision is for a policy or CVE source.
        kind: AcceptanceKind,
        /// Next required review; does not itself extend authorization.
        review_deadline: Option<NaiveDate>,
        /// Actual authorization expiry, if recorded by the source.
        authorization_expiry: Option<DateTime<Utc>>,
    },
}

/// Distinguishes policy decisions from CVE decisions.
pub enum AcceptanceKind {
    /// A policy waiver or deviation.
    Policy,
    /// A CVE host or environment disposition.
    Cve,
}

/// One exact link supplied by the authorized reader, not inferred by this writer.
pub struct Evidence<'a> {
    /// Stable finding UUID, when a finding is linked.
    pub finding_id: Option<Uuid>,
    /// Exact scan UUID, when scan evidence is linked.
    pub scan_id: Option<Uuid>,
    /// Source-supplied description of the link or scan.
    pub description: &'a str,
}

/// One stable source record in the authorized whole-scope snapshot.
pub struct Entry<'a> {
    /// Source UUID, independent of any display grouping.
    pub uuid: Uuid,
    /// Source's human-readable stable ID.
    pub source_id: &'a str,
    /// Source title.
    pub title: &'a str,
    /// Source description or justification.
    pub description: &'a str,
    /// Recorded lifecycle status, not a derived authorization decision.
    pub status: &'a str,
    /// Exact recorded source scope, including multiple or unspecified identities.
    pub scope: Scope<'a>,
    /// Optional canonical CVE identity recorded by the source.
    pub cve_id: Option<&'a str>,
    /// Source-specific dates and type.
    pub source: Source,
    /// All source-linked evidence; an empty slice produces one blank-link row.
    pub evidence: &'a [Evidence<'a>],
}

/// A caller-provided whole-scope result. A page of results is not a snapshot.
pub struct Snapshot<'a> {
    /// All authorized source records, before any display grouping or page cap.
    pub entries: &'a [Entry<'a>],
}

/// Downloadable bytes and their fixed, safe filename and content type.
#[derive(Debug)]
pub struct Download {
    /// Filename without user-controlled path components.
    pub filename: &'static str,
    /// Media type for an HTTP response.
    pub content_type: &'static str,
    /// Complete serialized payload.
    pub bytes: Vec<u8>,
}

/// Both tabular representations of the same source rows.
#[derive(Debug)]
pub struct Exports {
    /// UTF-8 CSV download.
    pub csv: Download,
    /// Office Open XML workbook download.
    pub xlsx: Download,
}

/// Writes complete, equivalent CSV and genuine XLSX from a preauthorized snapshot.
///
/// Every evidence link has its own row with the source UUID repeated. Unlinked
/// records get one row. For multi-scope plans, Scope type, Scope UUID, and Scope
/// name are three JSON arrays with matching indices, sorted by type then UUID.
/// Unspecified plan scope has type `unspecified` and blank UUID and name.
/// Acceptance records require exactly one system or environment scope.
/// A duplicate source UUID or an excessive count fails
/// rather than silently deduplicating, truncating, or exporting a partial result.
/// Spreadsheet-oriented strings are prefixed with an apostrophe when they could
/// be parsed as formulas; this transformation does not alter OSCAL source values.
///
/// # Errors
/// Returns an error for duplicate source UUIDs, blank required fields, more than
/// [`MAX_ITEMS`] sources or [`MAX_ROWS`] rows, Excel-length text, or workbook
/// serialization failure. No output is returned on error.
///
/// # Examples
/// ```ignore
/// let exports = write_register(&authorized_whole_scope_snapshot)?;
/// // The caller still enforces authorization before delivering either payload.
/// ```
pub fn write_register(snapshot: &Snapshot<'_>) -> Result<Exports> {
    ensure!(
        snapshot.entries.len() <= MAX_ITEMS,
        "register item limit exceeded"
    );
    let mut seen = HashSet::new();
    let mut count = 0usize;
    for entry in snapshot.entries {
        ensure!(
            seen.insert(entry.uuid),
            "duplicate register source UUID: {}",
            entry.uuid
        );
        for (name, value) in [
            ("source ID", entry.source_id),
            ("title", entry.title),
            ("status", entry.status),
        ] {
            ensure!(
                !value.trim().is_empty(),
                "register {} has no {name}",
                entry.uuid
            );
        }
        if let Some(cve) = entry.cve_id {
            ensure!(
                !cve.trim().is_empty(),
                "register {} has empty CVE ID",
                entry.uuid
            );
        }
        // SECURITY: Acceptance authority is always source-specific and exact.
        // Only plans can carry multiple or unspecified recorded scope identities.
        if matches!(entry.source, Source::Acceptance { .. }) {
            ensure!(
                matches!(
                    entry.scope,
                    Scope::System { .. } | Scope::Environment { .. }
                ),
                "acceptance requires one exact source scope"
            );
        }
        scope_cells(&entry.scope)?;
        count = count.saturating_add(entry.evidence.len().max(1));
        ensure!(count <= MAX_ROWS, "register row limit exceeded");
    }

    let mut csv = Vec::new();
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.set_name("POA&M CVE register")?;
    for (column, header) in HEADERS.iter().enumerate() {
        sheet.write_string(0, column as u16, *header)?;
    }
    csv_row(&mut csv, &HEADERS);
    let mut index = 1u32;
    for entry in snapshot.entries {
        let (kind, target, review, expiry) = match &entry.source {
            Source::Plan { target_date } => (
                "plan",
                target_date.map(|d| d.to_string()).unwrap_or_default(),
                String::new(),
                String::new(),
            ),
            Source::Acceptance {
                kind,
                review_deadline,
                authorization_expiry,
            } => (
                match kind {
                    AcceptanceKind::Policy => "policy decision",
                    AcceptanceKind::Cve => "CVE decision",
                },
                String::new(),
                review_deadline.map(|d| d.to_string()).unwrap_or_default(),
                authorization_expiry
                    .map(|d| d.to_rfc3339())
                    .unwrap_or_default(),
            ),
        };
        let (scope_kind, scope_id, scope_name) = scope_cells(&entry.scope)?;
        for evidence in entry
            .evidence
            .iter()
            .map(Some)
            .chain(std::iter::once(None).take(usize::from(entry.evidence.is_empty())))
        {
            let cells = [
                kind.to_owned(),
                entry.uuid.to_string(),
                entry.source_id.to_owned(),
                entry.title.to_owned(),
                entry.status.to_owned(),
                scope_kind.clone(),
                scope_id.clone(),
                scope_name.clone(),
                entry.cve_id.unwrap_or("").to_owned(),
                target.clone(),
                review.clone(),
                expiry.clone(),
                evidence
                    .and_then(|e| e.finding_id)
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
                evidence
                    .and_then(|e| e.scan_id)
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
                evidence.map(|e| e.description).unwrap_or("").to_owned(),
                entry.description.to_owned(),
            ];
            let safe = cells
                .iter()
                .map(|cell| safe_cell(cell))
                .collect::<Result<Vec<_>>>()?;
            for (column, value) in safe.iter().enumerate() {
                sheet.write_string(index, column as u16, value.as_str())?;
            }
            csv_row(&mut csv, &safe);
            index += 1;
        }
    }
    Ok(Exports {
        csv: Download {
            filename: "poam-cve-register.csv",
            content_type: "text/csv; charset=utf-8",
            bytes: csv,
        },
        xlsx: Download {
            filename: "poam-cve-register.xlsx",
            content_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            bytes: workbook.save_to_buffer()?,
        },
    })
}

fn scope_cells(scope: &Scope<'_>) -> Result<(String, String, String)> {
    let single = |kind: &str, id: Uuid, name: &str| -> Result<_> {
        ensure!(!name.trim().is_empty(), "register has no scope name");
        Ok((kind.to_owned(), id.to_string(), name.to_owned()))
    };
    match scope {
        Scope::System { id, name } => single("system", *id, name),
        Scope::Environment { id, name } => single("environment", *id, name),
        Scope::Unspecified => Ok(("unspecified".into(), String::new(), String::new())),
        Scope::Multiple { scopes } => {
            ensure!(
                scopes.len() >= 2,
                "multiple scope needs at least two identities"
            );
            let mut identities = Vec::with_capacity(scopes.len());
            for scope in *scopes {
                let (kind, id, name) = scope_cells(scope)?;
                ensure!(
                    kind != "unspecified" && kind != "multiple",
                    "invalid nested scope"
                );
                identities.push((kind, id, name));
            }
            identities.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
            ensure!(
                identities
                    .windows(2)
                    .all(|pair| pair[0].0 != pair[1].0 || pair[0].1 != pair[1].1),
                "duplicate recorded scope identity"
            );
            // INVARIANT: The three arrays share one sort order; each index is
            // one exact identity, not a cross-product of names and UUIDs.
            Ok((
                serde_json::to_string(
                    &identities
                        .iter()
                        .map(|(kind, _, _)| kind)
                        .collect::<Vec<_>>(),
                )?,
                serde_json::to_string(&identities.iter().map(|(_, id, _)| id).collect::<Vec<_>>())?,
                serde_json::to_string(
                    &identities
                        .iter()
                        .map(|(_, _, name)| name)
                        .collect::<Vec<_>>(),
                )?,
            ))
        }
    }
}

fn safe_cell(value: &str) -> Result<String> {
    // Excel and CSV consumers may skip whitespace or format controls before a
    // formula marker. Prefix the entire field, even when writing an XLSX string.
    let candidate = value.trim_start_matches(|c: char| {
        c.is_whitespace() || matches!(c, '\u{feff}' | '\u{200b}' | '\u{200c}' | '\u{200d}')
    });
    let unsafe_prefix = matches!(
        candidate.chars().next(),
        Some(
            '=' | '+' | '-' | '@' | '\u{ff1d}' | '\u{ff0b}' | '\u{ff0d}' | '\u{ff20}' | '\u{2212}'
        )
    );
    let result = if unsafe_prefix {
        format!("'{value}")
    } else {
        value.to_owned()
    };
    ensure!(
        result.chars().count() <= MAX_CELL_CHARS,
        "register cell exceeds Excel text limit"
    );
    Ok(result)
}

fn csv_row(out: &mut Vec<u8>, cells: &[impl AsRef<str>]) {
    for (column, cell) in cells.iter().enumerate() {
        if column != 0 {
            out.push(b',');
        }
        out.push(b'"');
        for byte in cell.as_ref().bytes() {
            if byte == b'"' {
                out.push(b'"');
            }
            out.push(byte);
        }
        out.push(b'"');
    }
    out.extend_from_slice(b"\r\n");
}
