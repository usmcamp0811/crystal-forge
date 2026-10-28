//! Representative-record selection for walkthrough stops.
//!
//! Some stops need a real record to point at, such as one CVE or one POA&M.
//! The coach reads those records through the same authenticated read APIs the
//! pages use. The server limits each response to what the signed-in person is
//! allowed to read.
//!
//! # Invariants
//!
//! - The coach never fabricates a record. When no suitable record exists, the
//!   stop reports [`Resolution::NoExample`] and explains what is missing.
//! - Selection is deterministic. It prefers an actionable record, then a
//!   truthful read-only record.
//! - Selection only reads. It never creates, updates or deletes anything.

use uuid::Uuid;

use super::tours::{CoachRole, Nav, PlanPick};
use crate::api::client;
use crate::api::models::{
    ComplianceBundleSystemsResponse, ComplianceControlStatus, ComplianceEvidenceResponse,
    ComplianceSystemRollup, CveFilters, CveListItem, HealthStatus, SystemSummary,
    SystemsListParams,
};
use crate::routes::Route;
use crate::views::poam_api::{
    self, AcceptanceEntry, AcceptanceSource, PoamListQuery, PoamRegisterSummary, PoamStatus,
};

/// A control the runner clicks after navigation, identified by a record key.
///
/// The opener must be a read-only `data-coach-open` control. The key matches
/// that control's `data-coach-key` attribute.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyedOpen {
    /// Opener identifier, matched against `data-coach-open`.
    pub opener: &'static str,
    /// Record key, matched against `data-coach-key`.
    pub key: String,
}

/// Where a stop goes.
#[derive(Clone, Debug, PartialEq)]
pub struct Destination {
    /// Typed route to navigate to.
    pub route: Route,
    /// Control to open after navigation, when the record is not addressable by
    /// route state.
    pub keyed_open: Option<KeyedOpen>,
}

/// Result of resolving a stop's destination.
#[derive(Clone, Debug, PartialEq)]
pub enum Resolution {
    /// Navigate to the destination.
    Go(Destination),
    /// No suitable record exists for the signed-in person.
    NoExample,
}

fn go(route: Route) -> Resolution {
    Resolution::Go(Destination {
        route,
        keyed_open: None,
    })
}

fn encode(value: &str) -> String {
    String::from(js_sys::encode_uri_component(value))
}

fn compliance_route(bundle: &str, version: &str, system: &str, policy: &str, view: &str) -> Route {
    Route::ComplianceView {
        bundle: bundle.to_string(),
        version: version.to_string(),
        system: system.to_string(),
        policy: policy.to_string(),
        poam: String::new(),
        view: view.to_string(),
    }
}

/// Picks the CVE and package a stop points at.
///
/// Prefers a critical, outstanding, patchable finding with Current exact
/// evidence. Falls back to any outstanding finding with Current evidence, then
/// any finding with Current evidence, then the first finding with a package.
pub fn pick_cve(items: &[CveListItem]) -> Option<(String, String)> {
    let with_package = || items.iter().filter(|item| item.package_name.is_some());
    let current = |item: &&CveListItem| item.inventory_counts().0 > 0;
    let outstanding = |item: &&CveListItem| item.triage_status == "outstanding";
    let chosen = with_package()
        .find(|item| is_actionable(item))
        .or_else(|| with_package().find(|item| outstanding(item) && current(item)))
        .or_else(|| with_package().find(current))
        .or_else(|| with_package().next())?;
    Some((chosen.cve_id.clone(), chosen.package_name.clone()?))
}

/// Returns whether a finding is the best walkthrough example.
///
/// A critical, outstanding, patchable finding with Current exact evidence can
/// show every triage control.
fn is_actionable(item: &CveListItem) -> bool {
    item.severity.eq_ignore_ascii_case("critical")
        && item.triage_status == "outstanding"
        && item.fix_status == "fix_available"
        && item.inventory_counts().0 > 0
}

/// Picks the system a stop opens.
///
/// Prefers a reachable system with critical CVEs, then the first system.
pub fn pick_system(systems: &[SystemSummary]) -> Option<Uuid> {
    systems
        .iter()
        .find(|system| {
            system.cve_counts.critical > 0 && system.health_status != HealthStatus::Offline
        })
        .or_else(|| systems.first())
        .map(|system| system.id)
}

/// Picks a host that the bundle applies to and that has failing controls.
pub fn pick_failing_system(
    response: &ComplianceBundleSystemsResponse,
) -> Option<&ComplianceSystemRollup> {
    response
        .systems
        .iter()
        .find(|system| system.applies && system.fail > 0)
}

/// Picks the first failing control on a host's evidence.
pub fn pick_failing_control(evidence: &ComplianceEvidenceResponse) -> Option<Uuid> {
    evidence
        .controls
        .iter()
        .find(|control| control.status == ComplianceControlStatus::Fail)
        .map(|control| control.policy_id)
}

/// Picks the remediation plan a stop points at.
///
/// `AwaitingVerification` prefers a plan in that state. Both picks fall back
/// to an unfinished plan, then any plan, so lifecycle controls still show.
pub fn pick_plan(items: &[PoamRegisterSummary], pick: PlanPick) -> Option<Uuid> {
    let status = |item: &&PoamRegisterSummary| item.summary.status;
    let unfinished = |item: &&PoamRegisterSummary| status(item) != PoamStatus::Completed;
    let chosen = match pick {
        PlanPick::AwaitingVerification => items
            .iter()
            .find(|item| status(item) == PoamStatus::AwaitingVerification)
            .or_else(|| items.iter().find(unfinished)),
        PlanPick::Open => items
            .iter()
            .find(|item| {
                status(item) == PoamStatus::InProgress
                    && item.summary.finding_count + item.summary.cve_finding_count > 1
            })
            .or_else(|| items.iter().find(unfinished)),
    };
    chosen.or_else(|| items.first()).map(|item| item.summary.id)
}

/// Picks the acceptance a stop opens.
///
/// Prefers an accepted CVE decision, then any accepted decision, then the
/// first decision.
pub fn pick_acceptance(items: &[AcceptanceEntry]) -> Option<&AcceptanceEntry> {
    let accepted = |item: &&AcceptanceEntry| item.status == "accepted";
    items
        .iter()
        .find(|item| accepted(item) && item.source != AcceptanceSource::PolicyWaiver)
        .or_else(|| items.iter().find(accepted))
        .or_else(|| items.first())
}

/// Reads at most two CVE lists. The second read only happens when the first,
/// critical-only list has no actionable finding.
async fn cve_destination() -> Result<Option<(String, String)>, String> {
    let critical = CveFilters {
        severity: Some("critical".into()),
        limit: Some(200),
        ..CveFilters::default()
    };
    let rows = client::fetch_cves(&critical)
        .await
        .map_err(|error| error.to_string())?;
    if rows.iter().any(is_actionable) {
        return Ok(pick_cve(&rows));
    }
    let all = CveFilters {
        limit: Some(200),
        ..CveFilters::default()
    };
    let rows = client::fetch_cves(&all)
        .await
        .map_err(|error| error.to_string())?;
    Ok(pick_cve(&rows))
}

/// Finds a bundle with a failing host. Reads at most `LIMIT` bundles.
async fn bundle_finding(
    need_control: bool,
) -> Result<Option<(Uuid, Option<Uuid>, Uuid, Option<Uuid>)>, String> {
    const LIMIT: usize = 8;
    let bundles = client::fetch_compliance_bundles()
        .await
        .map_err(|error| error.to_string())?;
    for bundle in bundles.iter().take(LIMIT) {
        let systems = client::fetch_compliance_bundle_systems(&bundle.id, None)
            .await
            .map_err(|error| error.to_string())?;
        if let Some(system) = pick_failing_system(&systems) {
            let policy = if need_control {
                let evidence = client::fetch_compliance_system_evidence(
                    &bundle.id,
                    &system.system_id,
                    systems.bundle_version_id.as_ref(),
                )
                .await
                .map_err(|error| error.to_string())?;
                pick_failing_control(&evidence)
            } else {
                None
            };
            if need_control && policy.is_none() {
                continue;
            }
            return Ok(Some((
                bundle.id,
                systems.bundle_version_id,
                system.system_id,
                policy,
            )));
        }
    }
    Ok(None)
}

/// Resolves a stop's destination from authorized read APIs.
///
/// # Errors
///
/// Returns the API error text when a read fails. The stop shows it instead of
/// guessing a record.
pub async fn resolve(nav: Nav, role: CoachRole) -> Result<Resolution, String> {
    Ok(match nav {
        Nav::Scanning => go(Route::ScanningView {}),
        Nav::Cves => go(Route::CvesView {
            query: String::new(),
        }),
        Nav::CveDrawer => match cve_destination().await? {
            Some((cve, package)) => go(Route::CvesView {
                query: format!("cve={}&cve_package={}", encode(&cve), encode(&package)),
            }),
            None => Resolution::NoExample,
        },
        Nav::SystemCves => {
            let page = client::fetch_systems(&SystemsListParams {
                per_page: Some(50),
                ..SystemsListParams::default()
            })
            .await
            .map_err(|error| error.to_string())?;
            match pick_system(&page.items) {
                Some(id) => go(Route::SystemDetailView {
                    id: id.to_string(),
                    tab: "cves".into(),
                    poam: String::new(),
                    config_mode: String::new(),
                    revision: String::new(),
                    generation: String::new(),
                    deploy_generation: String::new(),
                    cve_target: String::new(),
                    cve_mode: String::new(),
                }),
                None => Resolution::NoExample,
            }
        }
        Nav::Policies => go(Route::PoliciesView {}),
        Nav::Compliance => go(compliance_route("", "", "", "", "")),
        Nav::ComplianceBundle => match bundle_finding(false).await? {
            Some((bundle, version, _, _)) => go(compliance_route(
                &bundle.to_string(),
                &version.map(|id| id.to_string()).unwrap_or_default(),
                "",
                "",
                "",
            )),
            None => Resolution::NoExample,
        },
        Nav::ComplianceEvidence => match bundle_finding(true).await? {
            Some((bundle, version, system, policy)) => go(compliance_route(
                &bundle.to_string(),
                &version.map(|id| id.to_string()).unwrap_or_default(),
                &system.to_string(),
                &policy.map(|id| id.to_string()).unwrap_or_default(),
                "evidence",
            )),
            None => Resolution::NoExample,
        },
        Nav::EnvironmentAssignment => {
            if role != CoachRole::Admin {
                go(Route::EnvironmentsView {
                    query: String::new(),
                })
            } else {
                let environments = client::fetch_environments()
                    .await
                    .map_err(|error| error.to_string())?;
                let mut selected = None;
                for environment in environments {
                    let assignments = client::fetch_environment_assignments(&environment.id)
                        .await
                        .map_err(|error| error.to_string())?;
                    if assignments.iter().any(|assignment| {
                        assignment.active && assignment.scope_type == "environment"
                    }) {
                        selected = Some(environment.id);
                        break;
                    }
                }
                match selected {
                    Some(id) => Resolution::Go(Destination {
                        route: Route::EnvironmentsView {
                            query: String::new(),
                        },
                        keyed_open: Some(KeyedOpen {
                            opener: "env-edit",
                            key: id.to_string(),
                        }),
                    }),
                    None => Resolution::NoExample,
                }
            }
        }
        Nav::Poams => go(Route::PoamsView {
            query: String::new(),
        }),
        Nav::PoamsExport => {
            let plans = poam_api::list_poam_register(&PoamListQuery {
                limit: Some(1),
                ..PoamListQuery::default()
            })
            .await
            .map_err(|error| error.to_string())?;
            let acceptances = poam_api::list_acceptances(0, None, "")
                .await
                .map_err(|error| error.to_string())?;
            if !plans.items.is_empty() || acceptances.total > 0 {
                go(Route::PoamsView {
                    query: String::new(),
                })
            } else {
                Resolution::NoExample
            }
        }
        Nav::PoamsRa => go(Route::PoamsView {
            query: "kind=ra".into(),
        }),
        Nav::PoamPlan(pick) => {
            let query = PoamListQuery {
                limit: Some(50),
                ..PoamListQuery::default()
            };
            let page = poam_api::list_poam_register(&query)
                .await
                .map_err(|error| error.to_string())?;
            match pick_plan(&page.items, pick) {
                Some(id) => go(Route::PoamsView {
                    query: format!("poam={id}"),
                }),
                None => Resolution::NoExample,
            }
        }
        Nav::PoamsRaDetail => {
            let page = poam_api::list_acceptances(0, None, "")
                .await
                .map_err(|error| error.to_string())?;
            match pick_acceptance(&page.items) {
                Some(entry) => Resolution::Go(Destination {
                    route: Route::PoamsView {
                        query: "kind=ra".into(),
                    },
                    keyed_open: Some(KeyedOpen {
                        opener: "ra-row",
                        key: entry.source_id.to_string(),
                    }),
                }),
                None => Resolution::NoExample,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cve(id: &str, severity: &str, triage: &str, fix: &str, current: i64) -> CveListItem {
        serde_json::from_value(serde_json::json!({
            "cve_id": id,
            "severity": severity,
            "title": "title",
            "exploited": false,
            "package_name": format!("pkg-{id}"),
            "fix_status": fix,
            "affected_count": current,
            "current_affected_count": current,
            "affected_environments": null,
            "first_seen": null,
            "last_seen": null,
            "age_days": 1,
            "triage_status": triage,
            "cvss_v3_score": null,
            "cvss_vector": null,
            "published_date": null,
            "installed_version": null,
            "fixed_version": null,
        }))
        .unwrap()
    }

    #[test]
    fn cve_pick_prefers_an_actionable_critical_with_current_evidence() {
        let items = vec![
            cve("CVE-1", "high", "outstanding", "fix_available", 2),
            cve("CVE-2", "critical", "accepted", "fix_available", 2),
            cve("CVE-3", "critical", "outstanding", "fix_available", 0),
            cve("CVE-4", "critical", "outstanding", "fix_available", 3),
        ];
        assert_eq!(pick_cve(&items).unwrap().0, "CVE-4");
    }

    #[test]
    fn cve_pick_degrades_to_a_truthful_read_only_example() {
        let outstanding = vec![
            cve("CVE-1", "low", "accepted", "open", 1),
            cve("CVE-2", "low", "outstanding", "open", 1),
        ];
        assert_eq!(pick_cve(&outstanding).unwrap().0, "CVE-2");

        let current_only = vec![
            cve("CVE-1", "low", "accepted", "open", 0),
            cve("CVE-2", "low", "accepted", "open", 4),
        ];
        assert_eq!(pick_cve(&current_only).unwrap().0, "CVE-2");

        let no_current = vec![cve("CVE-9", "low", "accepted", "open", 0)];
        assert_eq!(pick_cve(&no_current).unwrap().0, "CVE-9");
    }

    #[test]
    fn cve_pick_reports_nothing_when_no_record_exists() {
        assert_eq!(pick_cve(&[]), None);
    }

    #[test]
    fn cve_pick_never_invents_a_package() {
        let mut item = cve("CVE-1", "critical", "outstanding", "fix_available", 1);
        item.package_name = None;
        assert_eq!(pick_cve(&[item]), None);
    }

    #[test]
    fn opener_keys_are_exact_record_identities() {
        let key = KeyedOpen {
            opener: "ra-row",
            key: Uuid::nil().to_string(),
        };
        assert_eq!(key.key.len(), 36);
    }
}
