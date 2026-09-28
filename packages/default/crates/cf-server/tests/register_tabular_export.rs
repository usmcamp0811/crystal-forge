// Standalone writer until an authorized whole-scope endpoint adopts it.
#[path = "../src/services/register_tabular_export.rs"]
mod writer;

use chrono::{NaiveDate, TimeZone, Utc};
use quick_xml::events::Event;
use std::io::{Cursor, Read};
use uuid::Uuid;
use writer::{AcceptanceKind, Entry, Evidence, Scope, Snapshot, Source, write_register};

fn record(id: u128, title: &'static str) -> Entry<'static> {
    Entry {
        uuid: Uuid::from_u128(id),
        source_id: "POAM-1",
        title,
        description: "Original justification",
        status: "scheduled",
        scope: Scope::Environment {
            id: Uuid::from_u128(9),
            name: "shared group",
        },
        cve_id: Some("CVE-2026-12345"),
        source: Source::Plan {
            target_date: Some(NaiveDate::from_ymd_opt(2026, 12, 1).unwrap()),
        },
        evidence: &[],
    }
}

fn csv_rows(bytes: &[u8]) -> Vec<Vec<String>> {
    csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(bytes)
        .records()
        .map(|r| r.unwrap().iter().map(str::to_owned).collect())
        .collect()
}

fn worksheet(bytes: &[u8]) -> Vec<Vec<String>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    assert!(archive.by_name("[Content_Types].xml").is_ok());
    assert!(archive.by_name("xl/workbook.xml").is_ok());
    let mut sheet = String::new();
    archive
        .by_name("xl/worksheets/sheet1.xml")
        .unwrap()
        .read_to_string(&mut sheet)
        .unwrap();
    let mut strings = String::new();
    if let Ok(mut shared) = archive.by_name("xl/sharedStrings.xml") {
        shared.read_to_string(&mut strings).unwrap();
    }
    let mut shared_values = Vec::new();
    let mut reader = quick_xml::Reader::from_str(&strings);
    let mut value = String::new();
    loop {
        match reader.read_event().unwrap() {
            Event::Start(tag) if tag.name().as_ref() == b"si" => value.clear(),
            Event::Text(text) => value.push_str(&text.unescape().unwrap()),
            Event::End(tag) if tag.name().as_ref() == b"si" => {
                shared_values.push(value.replace("_x000D_", "\r"))
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let mut reader = quick_xml::Reader::from_str(&sheet);
    let mut rows = Vec::new();
    let mut column = 0;
    let mut is_shared = false;
    let mut in_value = false;
    loop {
        match reader.read_event().unwrap() {
            Event::Start(tag) if tag.name().as_ref() == b"row" => {
                rows.push(vec![String::new(); 16])
            }
            Event::Start(tag) if tag.name().as_ref() == b"c" => {
                let reference = tag
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"r")
                    .unwrap();
                column = if reference.value[0] == b'P' {
                    15
                } else {
                    (reference.value[0] - b'A') as usize
                };
                is_shared = tag
                    .attributes()
                    .flatten()
                    .any(|a| a.key.as_ref() == b"t" && a.value.as_ref() == b"s");
            }
            Event::Start(tag) if tag.name().as_ref() == b"v" => in_value = true,
            Event::End(tag) if tag.name().as_ref() == b"v" => in_value = false,
            Event::Text(text) if in_value => {
                let raw = text.unescape().unwrap();
                rows.last_mut().unwrap()[column] = if is_shared {
                    shared_values[raw.parse::<usize>().unwrap()].clone()
                } else {
                    raw.into_owned()
                };
            }
            Event::Start(tag) | Event::Empty(tag) if tag.name().as_ref() == b"f" => {
                panic!("formula cell")
            }
            Event::Eof => break,
            _ => {}
        }
    }
    rows
}

#[test]
fn whole_scope_has_more_than_a_page_without_group_deduplication() {
    let entries: Vec<_> = (1..=102)
        .map(|id| record(id, "same display group"))
        .collect();
    let output = write_register(&Snapshot { entries: &entries }).unwrap();
    assert_eq!(output.csv.filename, "poam-cve-register.csv");
    assert_eq!(output.csv.content_type, "text/csv; charset=utf-8");
    assert_eq!(output.xlsx.filename, "poam-cve-register.xlsx");
    assert_eq!(
        output.xlsx.content_type,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    );
    let rows = csv_rows(&output.csv.bytes);
    assert_eq!(rows.len(), 103);
    assert_eq!(worksheet(&output.xlsx.bytes), rows);
    assert_eq!(rows[1][1], Uuid::from_u128(1).to_string());
    assert_eq!(rows[102][1], Uuid::from_u128(102).to_string());
    assert!(rows.iter().skip(1).all(|row| row[7] == "shared group"));

    // The writer does not expand caller scope: a filtered snapshot stays filtered.
    let filtered = write_register(&Snapshot {
        entries: &entries[100..],
    })
    .unwrap();
    assert_eq!(csv_rows(&filtered.csv.bytes).len(), 3);
    assert_eq!(
        worksheet(&filtered.xlsx.bytes),
        csv_rows(&filtered.csv.bytes)
    );
}

#[test]
fn evidence_and_acceptance_dates_have_equivalent_rows() {
    let evidence = [
        Evidence {
            finding_id: Some(Uuid::from_u128(11)),
            scan_id: Some(Uuid::from_u128(12)),
            description: "exact scan",
        },
        Evidence {
            finding_id: Some(Uuid::from_u128(11)),
            scan_id: Some(Uuid::from_u128(13)),
            description: "later scan",
        },
    ];
    let mut plan = record(1, "Plan");
    plan.evidence = &evidence;
    let acceptance = Entry {
        uuid: Uuid::from_u128(2),
        source_id: "DECISION-2",
        title: "Approved",
        description: "Recorded decision",
        status: "accepted",
        scope: Scope::System {
            id: Uuid::from_u128(7),
            name: "host-7",
        },
        cve_id: None,
        source: Source::Acceptance {
            kind: AcceptanceKind::Policy,
            review_deadline: Some(NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()),
            authorization_expiry: Some(Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap()),
        },
        evidence: &[],
    };
    let entries = [plan, acceptance];
    let output = write_register(&Snapshot { entries: &entries }).unwrap();
    let rows = csv_rows(&output.csv.bytes);
    assert_eq!(worksheet(&output.xlsx.bytes), rows);
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[1][12], rows[2][12]);
    assert_ne!(rows[1][13], rows[2][13]);
    assert_eq!(rows[3][0], "policy decision");
    assert_eq!(rows[3][10], "2026-10-01T00:00:00+00:00");
    assert_eq!(rows[3][11], "2027-01-01T00:00:00+00:00");
    assert_eq!(rows[3][7], "host-7");
    assert_eq!(rows[1][9], "2026-12-01");
    let cve = Entry {
        source: Source::Acceptance {
            kind: AcceptanceKind::Cve,
            review_deadline: None,
            authorization_expiry: None,
        },
        ..record(3, "CVE")
    };
    assert_eq!(
        csv_rows(
            &write_register(&Snapshot { entries: &[cve] })
                .unwrap()
                .csv
                .bytes
        )[1][0],
        "CVE decision"
    );
}

#[test]
fn csv_quotes_and_both_formats_neutralize_formulas() {
    for attack in [
        "=1+1",
        "+SUM(1)",
        "-2+3",
        "@cmd",
        " \t\n=HYPERLINK(1)",
        "\u{feff}=1",
        "\u{ff1d}1",
        "\u{ff0b}2",
        "\u{2212}3",
    ] {
        let entry = Entry {
            title: attack,
            description: "comma, quote \" and\r\nnew line",
            ..record(1, "ignored")
        };
        let out = write_register(&Snapshot { entries: &[entry] }).unwrap();
        let parsed = csv_rows(&out.csv.bytes);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[1][3], format!("'{attack}"));
        assert_eq!(parsed[1][15], "comma, quote \" and\r\nnew line");
        assert_eq!(worksheet(&out.xlsx.bytes), parsed);
    }
}

#[test]
fn rejects_overflow_and_duplicate_identity_instead_of_partial_output() {
    let entries: Vec<_> = (1..=10_001).map(|id| record(id, "item")).collect();
    assert!(
        write_register(&Snapshot { entries: &entries })
            .unwrap_err()
            .to_string()
            .contains("item limit")
    );
    let duplicated = [record(1, "one"), record(1, "two")];
    assert!(
        write_register(&Snapshot {
            entries: &duplicated
        })
        .unwrap_err()
        .to_string()
        .contains("duplicate")
    );
    let links: Vec<_> = (0..50_001)
        .map(|_| Evidence {
            finding_id: None,
            scan_id: None,
            description: "link",
        })
        .collect();
    let mut entry = record(1, "one");
    entry.evidence = &links;
    assert!(
        write_register(&Snapshot { entries: &[entry] })
            .unwrap_err()
            .to_string()
            .contains("row limit")
    );
    let long = "x".repeat(32_768);
    let entry = Entry {
        title: &long,
        ..record(1, "one")
    };
    assert!(
        write_register(&Snapshot { entries: &[entry] })
            .unwrap_err()
            .to_string()
            .contains("cell exceeds")
    );
    assert_eq!(
        csv_rows(
            &write_register(&Snapshot { entries: &[] })
                .unwrap()
                .csv
                .bytes
        )
        .len(),
        1
    );
}

#[test]
fn multiple_and_unspecified_scopes_round_trip_in_both_files() {
    let scopes = [
        Scope::System {
            id: Uuid::from_u128(12),
            name: "=host",
        },
        Scope::Environment {
            id: Uuid::from_u128(3),
            name: "group, 3",
        },
        Scope::System {
            id: Uuid::from_u128(2),
            name: "host 2",
        },
    ];
    let entries = [
        Entry {
            scope: Scope::Multiple { scopes: &scopes },
            ..record(1, "Many")
        },
        Entry {
            scope: Scope::Unspecified,
            ..record(2, "No scope")
        },
    ];
    let output = write_register(&Snapshot { entries: &entries }).unwrap();
    let rows = csv_rows(&output.csv.bytes);
    assert_eq!(worksheet(&output.xlsx.bytes), rows);
    assert_eq!(rows.len(), 3);
    let kinds: Vec<String> = serde_json::from_str(&rows[1][5]).unwrap();
    let ids: Vec<String> = serde_json::from_str(&rows[1][6]).unwrap();
    let names: Vec<String> = serde_json::from_str(&rows[1][7]).unwrap();
    assert_eq!(kinds, ["environment", "system", "system"]);
    assert_eq!(
        ids,
        [Uuid::from_u128(3), Uuid::from_u128(2), Uuid::from_u128(12)].map(|id| id.to_string())
    );
    assert_eq!(names, ["group, 3", "host 2", "=host"]);
    assert_eq!(&rows[2][5..8], ["unspecified", "", ""]);
    let duplicated = [
        Scope::System {
            id: Uuid::from_u128(1),
            name: "a",
        },
        Scope::System {
            id: Uuid::from_u128(1),
            name: "b",
        },
    ];
    assert!(
        write_register(&Snapshot {
            entries: &[Entry {
                scope: Scope::Multiple {
                    scopes: &duplicated
                },
                ..record(3, "duplicate")
            }]
        })
        .is_err()
    );
    let acceptance = Entry {
        scope: Scope::Unspecified,
        source: Source::Acceptance {
            kind: AcceptanceKind::Cve,
            review_deadline: None,
            authorization_expiry: None,
        },
        ..record(4, "acceptance")
    };
    assert!(
        write_register(&Snapshot {
            entries: &[acceptance]
        })
        .is_err()
    );
}
