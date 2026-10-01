//! Security walkthrough definitions and their pure presentation rules.
//!
//! The authoritative design is `docs/design/CrystalForge/components/CoachTours.jsx`.
//! This module holds the five walkthrough modules (A to E), their stops, role
//! gates and local progress rules. It has no browser, router or network
//! dependency, so every rule here is unit-testable.
//!
//! # Invariants
//!
//! - Walkthrough progress is presentation state only. It records which stops a
//!   person viewed in this browser. It never reflects the state of a finding,
//!   scan, acceptance or POA&M.
//! - A stop never describes a mutation the coach performs. Stops that point at
//!   a mutating control only teach. The runner opens read-only surfaces and
//!   never submits anything.
//! - Targets are `data-coach-target` identifiers. Openers are `data-coach-open`
//!   identifiers. The runner touches no other control.

use std::collections::BTreeMap;

use crate::api::models::AuthContext;
use crate::state::auth;

/// Role used to adapt walkthrough stops.
///
/// The role is derived from the authenticated session. It is never chosen in
/// the browser. Variants are ordered by permission level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CoachRole {
    /// Read-only access.
    Viewer,
    /// Can triage CVEs and manage remediation.
    Operator,
    /// Full administrative access, including setup and scan schedule.
    Admin,
}

impl CoachRole {
    /// Derives the highest role held by an authenticated session.
    ///
    /// Returns [`None`] when the session is not authenticated. An authenticated
    /// session with no recognized role is a [`CoachRole::Viewer`].
    pub fn from_auth(auth_context: &Option<AuthContext>) -> Option<Self> {
        if !auth::is_authenticated(auth_context) {
            return None;
        }
        Some(if auth::is_admin(auth_context) {
            Self::Admin
        } else if auth::is_operator_or_above(auth_context) {
            Self::Operator
        } else {
            Self::Viewer
        })
    }

    /// Returns the label shown in the role pill.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Viewer => "Viewer",
            Self::Operator => "Operator",
            Self::Admin => "Admin",
        }
    }

    /// Returns the label used inside a permission notice.
    pub const fn permission_label(self) -> &'static str {
        match self {
            Self::Viewer => "Viewer",
            Self::Operator => "Operator",
            Self::Admin => "Administrator",
        }
    }
}

/// Describes how a role can use a stop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopAccess {
    /// The role can use the destination and the action the stop explains.
    Open,
    /// The role can read the destination. The action the stop explains needs a
    /// higher role, so the stop shows a permission notice and opens nothing.
    Gated,
    /// The role cannot read the destination at all, so the coach does not
    /// navigate there. The stop shows a permission notice only.
    NoView,
}

/// Figures drawn inside a tour card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Figure {
    /// Scan schedule rules.
    Schedule,
    /// Current, scheduled and historical evidence tiers.
    Relations,
    /// System Detail evidence tabs.
    Tabs,
    /// Triage dispositions.
    Dispositions,
    /// Batch triage rules.
    Batch,
    /// Version assignments by scope.
    Assignments,
    /// Enforce versus report-only mode.
    Enforce,
    /// POA&M and acceptance work queues.
    Queues,
    /// POA&M lifecycle.
    Lifecycle,
    /// Verification rules.
    Verify,
    /// Baseline versus assessed exports.
    Exports,
    /// Human and source identities of an acceptance.
    Identities,
}

/// Plan a stop needs from the representative-record selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanPick {
    /// An open plan with several linked findings when one exists.
    Open,
    /// A plan awaiting verification when one exists.
    AwaitingVerification,
}

/// Typed destination for a stop.
///
/// The runner resolves each variant to a typed route. Variants that need a
/// record resolve it from authorized read APIs and never fabricate one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    /// The Scanning page.
    Scanning,
    /// The fleet CVE list.
    Cves,
    /// The fleet CVE list with one CVE and package drawer open.
    CveDrawer,
    /// System Detail on the CVEs tab for a representative host.
    SystemCves,
    /// The Policies page.
    Policies,
    /// The Compliance bundle catalog.
    Compliance,
    /// One compliance bundle drawer for a bundle with a failing host.
    ComplianceBundle,
    /// The per-control evidence drawer for a failing control.
    ComplianceEvidence,
    /// The environment edit surface for one authorized assignment.
    EnvironmentAssignment,
    /// The POA&M register, all record types.
    Poams,
    /// The POA&M register on the Risk acceptances tab.
    PoamsRa,
    /// The POA&M register export menu, when an exportable record exists.
    PoamsExport,
    /// One remediation plan detail.
    PoamPlan(PlanPick),
    /// One risk acceptance drawer.
    PoamsRaDetail,
}

/// One read-only step the runner performs before it looks for the target.
///
/// A step clicks the first enabled control carrying one of `openers` in its
/// `data-coach-open` attribute. The step is skipped when an element carrying
/// `satisfied_by` in `data-coach-target` is already on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Prep {
    /// Alternative opener identifiers, tried in order.
    pub openers: &'static [&'static str],
    /// Target identifier whose presence means the surface is already open.
    pub satisfied_by: Option<&'static str>,
}

/// One walkthrough stop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stop {
    /// Stable identifier such as `A1`. Progress records this value.
    pub id: &'static str,
    /// Stop title.
    pub title: &'static str,
    /// Why the surface exists.
    pub why: &'static str,
    /// What to do on the surface.
    pub doing: &'static str,
    /// Semantic warning shown in the Important block.
    pub important: Option<&'static str>,
    /// Figure drawn in the card.
    pub figure: Option<Figure>,
    /// Minimum role for the action this stop explains.
    pub gate: Option<CoachRole>,
    /// Text shown instead of `doing` when the role is below `gate`.
    pub gated_doing: Option<&'static str>,
    /// Note about actions reserved for administrators.
    pub admin_note: Option<&'static str>,
    /// Minimum role that can read the destination at all.
    ///
    /// PRODUCTION: the Scanning page and every Scanning API need the
    /// Administrator role, so Operators and Viewers must not be sent there.
    pub view_gate: Option<CoachRole>,
    /// Typed destination.
    pub nav: Nav,
    /// Read-only steps run before the target lookup.
    pub prep: &'static [Prep],
    /// Read-only steps used when the role is below `gate`.
    pub gated_prep: &'static [Prep],
    /// Target identifiers, tried in order.
    pub target: &'static [&'static str],
    /// Target identifiers used when the role is below `gate`.
    pub gated_target: &'static [&'static str],
    /// Explanation shown when the data a stop needs does not exist.
    pub no_example: Option<&'static str>,
}

impl Stop {
    /// Returns how `role` can use this stop.
    pub fn access(&self, role: CoachRole) -> StopAccess {
        if self.view_gate.is_some_and(|needed| role < needed) {
            StopAccess::NoView
        } else if self.gate.is_some_and(|needed| role < needed) {
            StopAccess::Gated
        } else {
            StopAccess::Open
        }
    }

    /// Returns the role label named in the permission notice for `role`.
    pub fn notice_role(&self, role: CoachRole) -> CoachRole {
        match self.access(role) {
            StopAccess::NoView => self.view_gate.unwrap_or(CoachRole::Admin),
            _ => self.gate.unwrap_or(CoachRole::Admin),
        }
    }

    /// Returns the steps to run for `role`.
    pub fn prep_for(&self, role: CoachRole) -> &'static [Prep] {
        if self.access(role) == StopAccess::Open {
            self.prep
        } else {
            self.gated_prep
        }
    }

    /// Returns the target identifiers to look for under `role`.
    pub fn target_for(&self, role: CoachRole) -> &'static [&'static str] {
        if self.access(role) == StopAccess::Open || self.gated_target.is_empty() {
            self.target
        } else {
            self.gated_target
        }
    }
}

/// One walkthrough module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SecurityModule {
    /// Module key, `A` to `E`.
    pub key: &'static str,
    /// Module title.
    pub title: &'static str,
    /// One-paragraph purpose shown on the module card.
    pub purpose: &'static str,
    /// Ordered stops.
    pub stops: &'static [Stop],
}

impl SecurityModule {
    /// Returns the number of stops whose action `role` cannot use.
    pub fn read_only_stops(&self, role: CoachRole) -> usize {
        self.stops
            .iter()
            .filter(|stop| stop.access(role) != StopAccess::Open)
            .count()
    }
}

/// Returns the module with `key`.
pub fn module(key: &str) -> Option<&'static SecurityModule> {
    SECURITY_MODULES
        .iter()
        .find(|candidate| candidate.key == key)
}

const fn open(openers: &'static [&'static str], satisfied_by: Option<&'static str>) -> Prep {
    Prep {
        openers,
        satisfied_by,
    }
}

const NO_PREP: &[Prep] = &[];

/// Opens the triage modal for the selected fleet CVE.
const OPEN_TRIAGE: Prep = open(&["cve-triage-open"], Some("cve-triage-modal"));
/// Selects the first non-empty quick-select group.
const SELECT_PAIRS: Prep = open(
    &[
        "cve-select-critical",
        "cve-select-high",
        "cve-select-patchable",
        "cve-select-outstanding",
    ],
    Some("cve-bulk-bar"),
);

/// The five security walkthrough modules, in design order.
pub static SECURITY_MODULES: [SecurityModule; 5] = [
    SecurityModule {
        key: "A",
        title: "Review vulnerability posture",
        purpose: "Where CVE evidence comes from, what scan status does and doesn't tell you, and which evidence tier you're reading.",
        stops: &[
            Stop {
                id: "A1",
                title: "Scan status is not a CVE result",
                why: "These counters describe the scan lifecycle: what is running, past its rescan interval, never scanned, or failed. Coverage is the share of tracked configurations that have a result.",
                doing: "Compare Failed and Never scanned against Coverage, then switch between the Active and Completed tabs below.",
                important: Some(
                    "A failed scan is not “no CVEs”, and an unscanned configuration is not clean. Both mean evidence is missing.",
                ),
                figure: None,
                gate: Some(CoachRole::Admin),
                // PRODUCTION: Scanning is Administrator-only, unlike the design.
                gated_doing: Some(
                    "The Scanning page is available to Administrators. Its counters describe the scan lifecycle, not CVE results: a failed scan is not “no CVEs”, and an unscanned configuration is not clean.",
                ),
                admin_note: None,
                view_gate: Some(CoachRole::Admin),
                nav: Nav::Scanning,
                prep: NO_PREP,
                gated_prep: NO_PREP,
                target: &["scan-stats"],
                gated_target: &[],
                no_example: None,
            },
            Stop {
                id: "A2",
                title: "Why evidence exists, or doesn't",
                why: "A scan record names the exact flake, commit and configuration it ran against, its lifecycle, and the scanner's own log.",
                doing: "Read the failure in Log, confirm the target identity in Details. Retry scan appears when the failure is retryable. Completed history can be archived and restored.",
                important: Some(
                    "Archiving hides a record from default lists. The result itself is retained.",
                ),
                figure: None,
                gate: Some(CoachRole::Admin),
                // PRODUCTION: Scanning is Administrator-only, unlike the design.
                gated_doing: Some(
                    "Administrators open a scan record to read its log and target identity. Archiving a completed scan hides it from default lists. The result itself is retained.",
                ),
                admin_note: None,
                view_gate: Some(CoachRole::Admin),
                nav: Nav::Scanning,
                prep: &[open(
                    &["scan-failed-tile", "scan-first-row"],
                    Some("scan-diagnostics"),
                )],
                gated_prep: NO_PREP,
                target: &["scan-diagnostics"],
                gated_target: &[],
                no_example: Some(
                    "No scan record is currently available to open. A scan record appears after Crystal Forge queues or runs a scan.",
                ),
            },
            Stop {
                id: "A3",
                title: "Scan schedule",
                why: "Scanning follows this policy, not every commit. Each row trades builder time against how fresh evidence stays for a class of configuration.",
                doing: "Read each control, then close without saving. The coach never saves the schedule.",
                important: None,
                figure: Some(Figure::Schedule),
                gate: Some(CoachRole::Admin),
                gated_doing: Some(
                    "Administrators set this schedule. You can see where it lives; the rules below explain why some configurations have older evidence than others.",
                ),
                admin_note: None,
                view_gate: Some(CoachRole::Admin),
                nav: Nav::Scanning,
                prep: &[open(&["scan-schedule"], Some("scan-schedule-modal"))],
                gated_prep: NO_PREP,
                target: &["scan-schedule-modal"],
                gated_target: &[],
                no_example: None,
            },
            Stop {
                id: "A4",
                title: "Fleet CVEs",
                why: "One row per CVE and canonical package: severity, whether a fixed version exists, triage state, and how many systems carry it.",
                doing: "Filter by severity or environment, then change Group to package to see which single upgrade removes the most exposure. Open any row for its detail drawer.",
                important: None,
                figure: None,
                gate: None,
                gated_doing: None,
                admin_note: None,
                view_gate: None,
                nav: Nav::Cves,
                prep: NO_PREP,
                gated_prep: NO_PREP,
                target: &["cve-stats"],
                gated_target: &[],
                no_example: None,
            },
            Stop {
                id: "A5",
                title: "Current, Scheduled, Historical",
                why: "Current exact evidence is the only tier that can be triaged. Scheduled and Historical evidence stay visible for planning and audit but are read-only.",
                doing: "Read which hosts sit in each tier before deciding anything about this CVE.",
                important: Some(
                    "Missing evidence does not mean clean. The same host can appear under Current and Scheduled when both exact configurations contain the package.",
                ),
                figure: Some(Figure::Relations),
                gate: None,
                gated_doing: None,
                admin_note: None,
                view_gate: None,
                nav: Nav::CveDrawer,
                prep: NO_PREP,
                gated_prep: NO_PREP,
                target: &["cve-relations"],
                gated_target: &[],
                no_example: Some(
                    "No CVE with visible evidence is currently available. Open this walkthrough again after a scan reports a vulnerable package.",
                ),
            },
            Stop {
                id: "A6",
                title: "Three kinds of evidence per system",
                why: "System Detail separates package vulnerabilities, hardening posture and compliance assessment. Each has its own source and its own remediation path.",
                doing: "Open CVEs, Hardening and Compliance in turn on this host.",
                important: Some(
                    "They are not interchangeable. A clean Hardening result says nothing about package CVEs.",
                ),
                figure: Some(Figure::Tabs),
                gate: None,
                gated_doing: None,
                admin_note: None,
                view_gate: None,
                nav: Nav::SystemCves,
                prep: NO_PREP,
                gated_prep: NO_PREP,
                target: &["system-tabs"],
                gated_target: &[],
                no_example: Some(
                    "No system is currently visible to you. Systems appear here after an administrator registers them.",
                ),
            },
        ],
    },
    SecurityModule {
        key: "B",
        title: "Triage vulnerabilities",
        purpose: "Deciding per environment whether to leave a CVE open, accept the risk, or schedule a patch — alone or in a batch.",
        stops: &[
            Stop {
                id: "B1",
                title: "Read the exact finding first",
                why: "The drawer shows the CVE, canonical package, CVSS, fixed version when the advisory publishes one, and the environments and hosts carrying it.",
                doing: "Confirm the fixed version and which environments hold Current evidence before you decide.",
                important: Some("Values the scanner did not report stay blank. Don't infer them."),
                figure: None,
                gate: None,
                gated_doing: None,
                admin_note: None,
                view_gate: None,
                nav: Nav::CveDrawer,
                prep: NO_PREP,
                gated_prep: NO_PREP,
                target: &["cve-stat-band"],
                gated_target: &[],
                no_example: Some(
                    "No CVE with visible evidence is currently available. Open this walkthrough again after a scan reports a vulnerable package.",
                ),
            },
            Stop {
                id: "B2",
                title: "Triage by environment",
                why: "Each environment gets its own disposition, so you can accept in dev and schedule a patch in production in one pass.",
                doing: "Pick a disposition per environment and read what each requires. Cancel when done; the coach never applies triage.",
                important: Some(
                    "Accepted risk is not remediation. The CVE stays listed and the finding does not pass.",
                ),
                figure: Some(Figure::Dispositions),
                gate: Some(CoachRole::Operator),
                gated_doing: Some(
                    "Triage requires Operator permission. The three dispositions below are what an Operator chooses between.",
                ),
                admin_note: None,
                view_gate: None,
                nav: Nav::CveDrawer,
                prep: &[OPEN_TRIAGE],
                gated_prep: NO_PREP,
                target: &["cve-triage-modal"],
                // PRODUCTION: the Triage button is not rendered below Operator,
                // so the gated stop points at the always-rendered triage status.
                gated_target: &["cve-triage-status"],
                no_example: Some(
                    "No CVE with current exact evidence is available to triage. Triage needs a Current exact finding.",
                ),
            },
            Stop {
                id: "B3",
                title: "Schedule patch opens a POA&M",
                why: "Scheduling needs a typed user or group assignee, a target completion and a remediation plan; standard patch milestones are prefilled. Crystal Forge derives the finding identity from the exact source evidence.",
                doing: "Look over the POA&M fields, then Cancel.",
                important: Some(
                    "POA&M status is not verification. The CVE stays open until newer exact scan evidence shows it gone.",
                ),
                figure: None,
                gate: Some(CoachRole::Operator),
                gated_doing: Some(
                    "Scheduling requires Operator permission and Current exact evidence.",
                ),
                admin_note: None,
                view_gate: None,
                nav: Nav::CveDrawer,
                prep: &[
                    OPEN_TRIAGE,
                    open(&["cve-triage-schedule"], Some("cve-triage-poam")),
                ],
                gated_prep: NO_PREP,
                target: &["cve-triage-modal"],
                gated_target: &["cve-triage-status"],
                no_example: Some(
                    "No CVE with current exact evidence is available to triage. Scheduling needs a Current exact finding.",
                ),
            },
            Stop {
                id: "B4",
                title: "Select exact pairs deliberately",
                why: "SELECT chips add Critical, High, Patchable or Outstanding rows; ⌘/Ctrl-click a package header to take a whole package. The bar counts selected exact pairs and distinct packages.",
                doing: "Build a selection that genuinely shares one decision, such as the patchable criticals in one package.",
                important: Some(
                    "Batch triage is for identical decisions. Different rationale belongs in a separate batch.",
                ),
                figure: None,
                gate: None,
                gated_doing: None,
                admin_note: None,
                view_gate: None,
                nav: Nav::Cves,
                prep: &[SELECT_PAIRS],
                gated_prep: NO_PREP,
                target: &["cve-bulk-bar", "cve-selection-strip"],
                gated_target: &[],
                no_example: Some("No CVE rows are available to select."),
            },
            Stop {
                id: "B5",
                title: "Triage N together",
                why: "One rationale is written into individually auditable decisions, one per exact pair.",
                doing: "Check the per-environment affected counts and the grouping choice, then Cancel.",
                important: Some("The batch commits atomically or not at all."),
                figure: Some(Figure::Batch),
                gate: Some(CoachRole::Operator),
                gated_doing: Some("Batch triage requires Operator permission."),
                admin_note: None,
                view_gate: None,
                nav: Nav::Cves,
                prep: &[
                    SELECT_PAIRS,
                    open(&["cve-batch-open"], Some("cve-batch-modal")),
                ],
                gated_prep: &[SELECT_PAIRS],
                target: &["cve-batch-modal"],
                gated_target: &["cve-bulk-bar"],
                no_example: Some("No CVE rows are available to select."),
            },
        ],
    },
    SecurityModule {
        key: "C",
        title: "Review compliance evidence",
        purpose: "How policies, bundles, versioned assignments and enforcement modes produce the per-control evidence you act on.",
        stops: &[
            Stop {
                id: "C1",
                title: "Policies are individual criteria",
                why: "Security controls carry framework identity. Group them by NIST 800-53 family, STIG severity (CAT), CCI, SRG category, CMMC level, CIS section or remediation status.",
                doing: "Open the Security controls tab, then change the grouping and watch the same controls regroup.",
                important: None,
                figure: None,
                gate: None,
                gated_doing: None,
                // PRODUCTION: Import is disabled below Administrator. The New
                // custom policy button is not gated in the interface.
                admin_note: Some("Importing policies is an administrator action."),
                view_gate: None,
                nav: Nav::Policies,
                prep: NO_PREP,
                gated_prep: NO_PREP,
                target: &["policy-domain-tabs"],
                gated_target: &[],
                no_example: None,
            },
            Stop {
                id: "C2",
                title: "Bundles organize policies into a baseline",
                why: "Each bundle has a lineage, numbered versions, a publication state, a framework, a score and a set of systems in scope.",
                doing: "Scan the list for publication state and version before comparing scores.",
                important: None,
                figure: None,
                gate: None,
                gated_doing: None,
                // PRODUCTION: Export XCCDF is open to every role.
                admin_note: Some("Import STIG / XCCDF and New bundle are administrator actions."),
                view_gate: None,
                nav: Nav::Compliance,
                prep: NO_PREP,
                gated_prep: NO_PREP,
                target: &["compliance-head"],
                gated_target: &[],
                no_example: None,
            },
            Stop {
                id: "C3",
                title: "Assignments pin versions by scope",
                why: "A bundle is not applied globally. Each environment's settings pick an exact bundle version, so production can stay on one revision while staging assesses the next. A system can carry its own pin.",
                // PRODUCTION: the environment form lists assigned bundles with
                // their version. It has no per-version picker.
                doing: "Read each assigned bundle and the version it is pinned to. Close without saving; the coach never changes an assignment.",
                important: None,
                figure: Some(Figure::Assignments),
                gate: Some(CoachRole::Admin),
                gated_doing: Some(
                    "Administrators set assignments. You can read the assigned version on the environment and in each bundle's header.",
                ),
                admin_note: None,
                view_gate: None,
                nav: Nav::EnvironmentAssignment,
                prep: NO_PREP,
                gated_prep: NO_PREP,
                target: &["env-bundle-assignment"],
                gated_target: &["env-page-head"],
                no_example: Some(
                    "No environment is currently available to open. Environments appear after an administrator creates them.",
                ),
            },
            Stop {
                id: "C4",
                title: "Enforce vs Report only",
                why: "Each environment's bundle assignment picks a mode here in its settings. The mode decides whether a failure can block deployment. It never decides whether the failure is real.",
                doing: "Toggle Enforce and Report only to compare what each does, then close without saving. The coach never changes an assignment.",
                important: Some(
                    "Report-only FAIL is still FAIL. Creating a POA&M does not change FAIL to PASS.",
                ),
                figure: Some(Figure::Enforce),
                gate: Some(CoachRole::Admin),
                gated_doing: Some(
                    "Administrators set the assignment mode. Operators and Viewers read it on the environment.",
                ),
                admin_note: None,
                view_gate: None,
                nav: Nav::EnvironmentAssignment,
                prep: NO_PREP,
                gated_prep: NO_PREP,
                target: &["env-assignment-mode", "env-bundle-assignment"],
                gated_target: &["env-page-head"],
                no_example: Some(
                    "No environment is currently available to open. Environments appear after an administrator creates them.",
                ),
            },
            Stop {
                id: "C5",
                title: "System matrix",
                why: "Every host in scope with its pass, warning and fail counts and any linked POA&M.",
                // PRODUCTION: the matrix filters are All, Clean, Warning and
                // Failing. The bundle POA&M view counts failures with no POA&M.
                doing: "Filter to Failing, then open a failing host. The bundle's POA&M view counts failures that have no POA&M yet.",
                important: None,
                figure: None,
                gate: None,
                gated_doing: None,
                admin_note: None,
                view_gate: None,
                nav: Nav::ComplianceBundle,
                prep: NO_PREP,
                gated_prep: NO_PREP,
                target: &["bundle-systems"],
                gated_target: &[],
                no_example: Some(
                    "No compliance bundle is currently available. A bundle appears after an administrator creates or imports one.",
                ),
            },
            Stop {
                id: "C6",
                title: "Per-control evidence",
                why: "This is the authoritative place to learn why a control fails: framework release, requirement, policy, exact host, result, source evidence and any remediation already linked.",
                doing: "Step through the controls and read the source evidence, not just the result chip.",
                important: None,
                figure: None,
                gate: None,
                gated_doing: None,
                admin_note: None,
                view_gate: None,
                nav: Nav::ComplianceEvidence,
                prep: NO_PREP,
                gated_prep: NO_PREP,
                target: &["evidence-drawer"],
                gated_target: &[],
                no_example: Some(
                    "No assessed host is currently available. Evidence appears after a system is assessed against a bundle.",
                ),
            },
            Stop {
                id: "C7",
                title: "Create remediation from the finding",
                why: "A POA&M created here is linked to this finding, so ownership and verification follow the evidence.",
                doing: "Find the POA&M action on a failing control. Open it to see what it would record, then cancel.",
                important: Some(
                    "Don't create an unrelated free-floating plan to work around finding ownership. A failing report-only finding still qualifies for a POA&M.",
                ),
                figure: None,
                gate: Some(CoachRole::Operator),
                gated_doing: Some("Creating a POA&M requires Operator permission."),
                admin_note: None,
                view_gate: None,
                nav: Nav::ComplianceEvidence,
                prep: &[open(&["finding-poam-form"], Some("poam-create-dialog"))],
                gated_prep: NO_PREP,
                target: &["poam-create-dialog"],
                gated_target: &["evidence-drawer"],
                no_example: Some(
                    "No failing control is currently available. A POA&M can be created from a failing control that has no active plan.",
                ),
            },
        ],
    },
    SecurityModule {
        key: "D",
        title: "Manage remediation and accepted risk",
        purpose: "Working the POA&M register: plans, acceptances, queues, lifecycle, verification and renewals.",
        stops: &[
            Stop {
                id: "D1",
                title: "One register, two kinds of record",
                why: "Remediation plan: a commitment to fix. Risk acceptance: a decision to let the finding stand for now. They cover the same findings but are different source records.",
                doing: "Switch between Everything, Remediation plans and Risk acceptances.",
                important: None,
                figure: None,
                gate: None,
                gated_doing: None,
                admin_note: None,
                view_gate: None,
                nav: Nav::Poams,
                prep: NO_PREP,
                gated_prep: NO_PREP,
                target: &["register-kinds"],
                gated_target: &[],
                no_example: None,
            },
            Stop {
                id: "D2",
                title: "Work queues",
                why: "Queues sort records by what needs a person next: overdue, due in 14 days, awaiting verification, blocked, quiet, unassigned; for acceptances, expired, review soon, no review date.",
                doing: "Click a queue to filter the list. Click again to clear it.",
                important: Some("Queues are operator work lists. They are not evidence outcomes."),
                figure: Some(Figure::Queues),
                gate: None,
                gated_doing: None,
                admin_note: None,
                view_gate: None,
                nav: Nav::Poams,
                prep: NO_PREP,
                gated_prep: NO_PREP,
                target: &["register-main", "register-queues"],
                gated_target: &[],
                no_example: None,
            },
            Stop {
                id: "D3",
                title: "Scope and grouping",
                why: "Browse by Environment, Bundle or Owner / Approver. Chips narrow the list; Focus drills into one group without losing the rest.",
                doing: "Pick an environment chip, then clear it.",
                important: Some(
                    "A host-only source decision that recorded no environment is shown without one. Crystal Forge doesn't infer it.",
                ),
                figure: None,
                gate: None,
                gated_doing: None,
                admin_note: None,
                view_gate: None,
                nav: Nav::Poams,
                prep: NO_PREP,
                gated_prep: NO_PREP,
                target: &["register-scope"],
                gated_target: &[],
                no_example: None,
            },
            Stop {
                id: "D4",
                title: "Remediation plan detail",
                why: "POAM-#### is the human ID. The plan carries linked findings, owner, target completion, risk, milestones and history.",
                doing: "Read the linked findings table: each row keeps its own exact host and evidence identity.",
                important: Some(
                    "A POA&M can group several CVE findings when someone grouped them explicitly. Grouping never merges their evidence.",
                ),
                figure: None,
                gate: None,
                gated_doing: None,
                admin_note: None,
                view_gate: None,
                nav: Nav::PoamPlan(PlanPick::Open),
                prep: NO_PREP,
                gated_prep: NO_PREP,
                target: &["poam-tray"],
                gated_target: &[],
                no_example: Some(
                    "No remediation plan is currently available. Plans appear after someone creates a POA&M from a finding or schedules a CVE patch.",
                ),
            },
            Stop {
                id: "D5",
                title: "Lifecycle",
                why: "Open, In progress, Blocked and Awaiting verification record where the work is. Awaiting verification means remediation is reported complete; the technical finding is still independent.",
                doing: "Look at the status control. Changing status records workflow only.",
                important: Some("POA&M status is not verification."),
                figure: Some(Figure::Lifecycle),
                gate: Some(CoachRole::Operator),
                gated_doing: Some("Changing status requires Operator permission."),
                admin_note: None,
                view_gate: None,
                nav: Nav::PoamPlan(PlanPick::AwaitingVerification),
                prep: NO_PREP,
                gated_prep: NO_PREP,
                target: &["poam-lifecycle"],
                gated_target: &[],
                no_example: Some(
                    "No remediation plan is currently available. Plans appear after someone creates a POA&M from a finding or schedules a CVE patch.",
                ),
            },
            Stop {
                id: "D6",
                title: "Verify now, then close",
                why: "Verify now checks authoritative current evidence for every linked finding. Authoritative close is only offered once that check passes.",
                // PRODUCTION: Verify now records a verification result, so the
                // design wording "safe to run" is not repeated here.
                doing: "Read what verification requires below. The coach never runs Verify now and never closes the POA&M.",
                important: None,
                figure: Some(Figure::Verify),
                gate: Some(CoachRole::Operator),
                gated_doing: Some("Verification and closure require Operator permission."),
                admin_note: None,
                view_gate: None,
                nav: Nav::PoamPlan(PlanPick::AwaitingVerification),
                prep: NO_PREP,
                gated_prep: NO_PREP,
                target: &["poam-verify", "poam-lifecycle"],
                gated_target: &[],
                no_example: Some(
                    "No remediation plan is currently available. Plans appear after someone creates a POA&M from a finding or schedules a CVE patch.",
                ),
            },
            Stop {
                id: "D7",
                title: "Risk acceptance register",
                why: "RA-#### identifies one operator-facing renewal chain. The exact source decision UUID stays the machine and audit identity.",
                doing: "Compare review dates and approvers across acceptances.",
                important: Some(
                    "A renewal can create a new immutable source decision under the same RA number. An unrelated new acceptance gets a new RA number.",
                ),
                figure: None,
                gate: None,
                gated_doing: None,
                admin_note: None,
                view_gate: None,
                nav: Nav::PoamsRa,
                prep: NO_PREP,
                gated_prep: NO_PREP,
                target: &["register-kinds"],
                gated_target: &[],
                no_example: None,
            },
            Stop {
                id: "D8",
                title: "Risk acceptance detail",
                why: "Source type, subject, approver, approval time, review deadline, exact scope and justification, plus the source-record identity.",
                // PRODUCTION: the drawer heading names the source type. The
                // exact decision UUID sits in the Source record identity block.
                doing: "Open Source record identity to see the exact decision UUID behind this RA number.",
                important: Some(
                    "Risk acceptance records a decision. It does not make a finding pass or mark it remediated.",
                ),
                figure: None,
                gate: None,
                gated_doing: None,
                admin_note: None,
                view_gate: None,
                nav: Nav::PoamsRaDetail,
                prep: NO_PREP,
                gated_prep: NO_PREP,
                target: &["ra-truth", "ra-drawer"],
                gated_target: &[],
                no_example: Some(
                    "No risk acceptance is currently available. Acceptances appear after an operator accepts risk for a CVE or policy finding.",
                ),
            },
            Stop {
                id: "D9",
                title: "Re-review or convert",
                why: "Re-review renews the acceptance for 90 days. Convert to POA&M starts remediation without rewriting the original evidence; the acceptance stays in audit history.",
                doing: "Decide which applies. The coach won't renew or convert.",
                important: Some(
                    "There is no universal revoke. Changing course means renewing, converting, or a new decision.",
                ),
                figure: None,
                gate: Some(CoachRole::Operator),
                gated_doing: Some(
                    "CVE acceptance changes require Operator permission; policy-waiver renewal and conversion can require Admin.",
                ),
                admin_note: None,
                view_gate: None,
                nav: Nav::PoamsRaDetail,
                prep: NO_PREP,
                gated_prep: NO_PREP,
                target: &["ra-footer", "ra-drawer"],
                gated_target: &["ra-drawer"],
                no_example: Some(
                    "No risk acceptance is currently available. Acceptances appear after an operator accepts risk for a CVE or policy finding.",
                ),
            },
        ],
    },
    SecurityModule {
        key: "E",
        title: "Prepare audit evidence",
        purpose: "Which export answers which auditor question, and how acceptance identities survive into OSCAL.",
        stops: &[
            Stop {
                id: "E1",
                title: "Baseline vs assessed evidence",
                why: "A bundle's XCCDF export is the baseline definition: what is checked. The evidence package is assessed results: what was found, on which hosts, with POA&Ms and acceptances.",
                doing: "Use Import / Export for a bundle's XCCDF; use Export evidence package for an environment or host set.",
                important: None,
                figure: Some(Figure::Exports),
                gate: None,
                gated_doing: None,
                admin_note: None,
                view_gate: None,
                nav: Nav::Compliance,
                prep: NO_PREP,
                gated_prep: NO_PREP,
                target: &["compliance-export-evidence"],
                gated_target: &[],
                no_example: None,
            },
            Stop {
                id: "E2",
                title: "Register export",
                why: "The register exports as OSCAL JSON, Excel, CSV or OSCAL XML. It contains remediation plans and risk acceptances according to the record type and scope you have selected.",
                doing: "Set the record tab and scope first, then export.",
                important: None,
                figure: None,
                gate: None,
                gated_doing: None,
                admin_note: None,
                view_gate: None,
                nav: Nav::PoamsExport,
                prep: &[open(&["register-export"], Some("register-export-menu"))],
                gated_prep: NO_PREP,
                target: &["register-export-menu"],
                gated_target: &[],
                no_example: Some(
                    "The register export needs at least one record. Records appear after someone creates a POA&M or accepts risk.",
                ),
            },
            Stop {
                id: "E3",
                title: "Two identities per acceptance",
                why: "RA-0042 is the human lifecycle. The typed source and UUID are the exact immutable decision.",
                doing: "When reconciling an OSCAL export, match on source-id, not the RA number.",
                important: Some("RA-#### never replaces the source UUID."),
                figure: Some(Figure::Identities),
                gate: None,
                gated_doing: None,
                admin_note: None,
                view_gate: None,
                nav: Nav::PoamsRaDetail,
                prep: NO_PREP,
                gated_prep: NO_PREP,
                target: &["ra-source-toggle", "ra-source-id"],
                gated_target: &[],
                no_example: Some(
                    "No risk acceptance is currently available. Acceptances appear after an operator accepts risk for a CVE or policy finding.",
                ),
            },
        ],
    },
];

/// Walkthrough progress in this browser.
///
/// Maps a module key to the stop identifiers the person has viewed. The value
/// is presentation state only.
pub type Progress = BTreeMap<String, Vec<String>>;

/// Presentation state of one module card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleState {
    /// No stop has been viewed.
    NotStarted,
    /// Some stops have been viewed.
    InProgress {
        /// Number of distinct stops viewed.
        seen: usize,
        /// Number of stops in the module.
        total: usize,
    },
    /// Every stop has been viewed.
    Completed,
}

/// Returns the distinct viewed stops of `module` that still exist.
///
/// Ignores identifiers that no longer name a stop, so a stale browser record
/// from an older release cannot inflate progress.
fn viewed_count(module: &SecurityModule, progress: &Progress) -> usize {
    let viewed = progress.get(module.key);
    module
        .stops
        .iter()
        .filter(|stop| viewed.is_some_and(|ids| ids.iter().any(|id| id == stop.id)))
        .count()
}

/// Returns the presentation state of `module`.
pub fn module_state(module: &SecurityModule, progress: &Progress) -> ModuleState {
    let seen = viewed_count(module, progress);
    let total = module.stops.len();
    if seen == 0 {
        ModuleState::NotStarted
    } else if seen >= total {
        ModuleState::Completed
    } else {
        ModuleState::InProgress { seen, total }
    }
}

/// Returns the index of the first stop not yet viewed, or zero.
pub fn resume_index(module: &SecurityModule, progress: &Progress) -> usize {
    let viewed = progress.get(module.key);
    module
        .stops
        .iter()
        .position(|stop| !viewed.is_some_and(|ids| ids.iter().any(|id| id == stop.id)))
        .unwrap_or(0)
}

/// Records that `stop_id` was viewed, keeping identifiers unique.
pub fn mark_viewed(progress: &mut Progress, module_key: &str, stop_id: &str) {
    let ids = progress.entry(module_key.to_string()).or_default();
    if !ids.iter().any(|id| id == stop_id) {
        ids.push(stop_id.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::{AuthContext, AuthMode, Role};

    fn auth(roles: Vec<Role>, authenticated: bool) -> Option<AuthContext> {
        Some(AuthContext {
            is_authenticated: authenticated,
            user: None,
            roles,
            auth_mode: AuthMode::Local,
        })
    }

    #[test]
    fn role_comes_from_the_session_with_the_highest_role_winning() {
        assert_eq!(CoachRole::from_auth(&None), None);
        assert_eq!(CoachRole::from_auth(&auth(vec![Role::Admin], false)), None);
        assert_eq!(
            CoachRole::from_auth(&auth(vec![Role::Viewer, Role::Operator], true)),
            Some(CoachRole::Operator)
        );
        assert_eq!(
            CoachRole::from_auth(&auth(vec![Role::Viewer, Role::Admin], true)),
            Some(CoachRole::Admin)
        );
        assert_eq!(
            CoachRole::from_auth(&auth(vec![], true)),
            Some(CoachRole::Viewer)
        );
    }

    #[test]
    fn modules_match_the_design_order_and_stop_counts() {
        let shape: Vec<(&str, usize)> = SECURITY_MODULES
            .iter()
            .map(|module| (module.key, module.stops.len()))
            .collect();
        assert_eq!(shape, [("A", 6), ("B", 5), ("C", 7), ("D", 9), ("E", 3)]);
        assert_eq!(
            SECURITY_MODULES.map(|module| module.title),
            [
                "Review vulnerability posture",
                "Triage vulnerabilities",
                "Review compliance evidence",
                "Manage remediation and accepted risk",
                "Prepare audit evidence",
            ]
        );
        for module in &SECURITY_MODULES {
            for (index, stop) in module.stops.iter().enumerate() {
                assert_eq!(stop.id, format!("{}{}", module.key, index + 1));
            }
        }
    }

    #[test]
    fn stop_ids_are_unique_and_every_stop_has_a_target() {
        let mut seen = std::collections::BTreeSet::new();
        for module in &SECURITY_MODULES {
            for stop in module.stops {
                assert!(seen.insert(stop.id), "duplicate stop id {}", stop.id);
                assert!(
                    !stop.target.is_empty(),
                    "{} needs at least one target",
                    stop.id
                );
                assert!(!stop.title.is_empty() && !stop.why.is_empty() && !stop.doing.is_empty());
            }
        }
        assert_eq!(seen.len(), 30);
    }

    #[test]
    fn every_gated_stop_explains_the_permission_requirement() {
        for module in &SECURITY_MODULES {
            for stop in module.stops {
                if stop.gate.is_some() {
                    assert!(
                        stop.gated_doing.is_some(),
                        "{} is gated but has no permission explanation",
                        stop.id
                    );
                }
            }
        }
    }

    #[test]
    fn mutation_stops_open_nothing_for_roles_below_the_gate() {
        let triage = module("B")
            .unwrap()
            .stops
            .iter()
            .find(|s| s.id == "B2")
            .unwrap();
        assert_eq!(triage.access(CoachRole::Viewer), StopAccess::Gated);
        assert_eq!(triage.access(CoachRole::Operator), StopAccess::Open);
        assert!(triage.prep_for(CoachRole::Viewer).is_empty());
        assert_eq!(triage.target_for(CoachRole::Viewer), &["cve-triage-status"]);
        assert_eq!(
            triage.target_for(CoachRole::Operator),
            &["cve-triage-modal"]
        );

        let batch = module("B")
            .unwrap()
            .stops
            .iter()
            .find(|s| s.id == "B5")
            .unwrap();
        // A Viewer can still build a selection. The batch dialog stays closed.
        assert_eq!(batch.prep_for(CoachRole::Viewer), &[SELECT_PAIRS]);
        assert_eq!(batch.target_for(CoachRole::Viewer), &["cve-bulk-bar"]);
    }

    #[test]
    fn scanning_is_not_opened_for_non_administrators() {
        for id in ["A1", "A2", "A3"] {
            let stop = module("A")
                .unwrap()
                .stops
                .iter()
                .find(|s| s.id == id)
                .unwrap();
            assert_eq!(stop.access(CoachRole::Viewer), StopAccess::NoView, "{id}");
            assert_eq!(stop.access(CoachRole::Operator), StopAccess::NoView, "{id}");
            assert_eq!(stop.access(CoachRole::Admin), StopAccess::Open, "{id}");
            assert_eq!(stop.notice_role(CoachRole::Viewer), CoachRole::Admin);
            assert!(stop.prep_for(CoachRole::Operator).is_empty());
        }
        assert_eq!(module("A").unwrap().read_only_stops(CoachRole::Viewer), 3);
        assert_eq!(module("A").unwrap().read_only_stops(CoachRole::Admin), 0);
    }

    #[test]
    fn admin_gated_environment_stops_stay_readable_for_other_roles() {
        for id in ["C3", "C4"] {
            let stop = module("C")
                .unwrap()
                .stops
                .iter()
                .find(|s| s.id == id)
                .unwrap();
            assert_eq!(stop.access(CoachRole::Operator), StopAccess::Gated);
            assert!(stop.prep_for(CoachRole::Operator).is_empty());
            assert_eq!(stop.target_for(CoachRole::Operator), &["env-page-head"]);
            assert_eq!(stop.access(CoachRole::Admin), StopAccess::Open);
        }
    }

    #[test]
    fn prep_steps_use_only_open_only_identifiers() {
        // The runner clicks only `data-coach-open` controls named here. None
        // of these identifiers may name a submit, save, apply or close action.
        let forbidden = [
            "save", "submit", "apply", "create", "accept", "renew", "convert", "verify", "close",
            "revoke", "delete", "archive", "restore", "retry",
        ];
        for module in &SECURITY_MODULES {
            for stop in module.stops {
                for step in stop.prep.iter().chain(stop.gated_prep) {
                    for opener in step.openers {
                        for word in forbidden {
                            assert!(
                                !opener.contains(word),
                                "{} opener {opener} names a mutation ({word})",
                                stop.id
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn progress_records_only_viewed_stops_and_resumes_at_the_first_unseen() {
        let module_a = module("A").unwrap();
        let mut progress = Progress::new();
        assert_eq!(module_state(module_a, &progress), ModuleState::NotStarted);
        assert_eq!(resume_index(module_a, &progress), 0);

        mark_viewed(&mut progress, "A", "A1");
        mark_viewed(&mut progress, "A", "A1");
        mark_viewed(&mut progress, "A", "A2");
        assert_eq!(progress["A"], ["A1", "A2"]);
        assert_eq!(
            module_state(module_a, &progress),
            ModuleState::InProgress { seen: 2, total: 6 }
        );
        assert_eq!(resume_index(module_a, &progress), 2);

        for stop in module_a.stops {
            mark_viewed(&mut progress, "A", stop.id);
        }
        assert_eq!(module_state(module_a, &progress), ModuleState::Completed);
        // A completed module restarts from the beginning.
        assert_eq!(resume_index(module_a, &progress), 0);
    }

    #[test]
    fn stale_progress_identifiers_do_not_count() {
        let module_e = module("E").unwrap();
        let mut progress = Progress::new();
        progress.insert("E".into(), vec!["E1".into(), "E9".into(), "Z1".into()]);
        assert_eq!(
            module_state(module_e, &progress),
            ModuleState::InProgress { seen: 1, total: 3 }
        );
    }

    #[test]
    fn security_teaching_points_are_preserved() {
        let all: String = SECURITY_MODULES
            .iter()
            .flat_map(|module| module.stops)
            .flat_map(|stop| {
                [
                    stop.why,
                    stop.doing,
                    stop.important.unwrap_or(""),
                    stop.gated_doing.unwrap_or(""),
                ]
            })
            .collect::<Vec<_>>()
            .join("\n");
        for point in [
            "A failed scan is not “no CVEs”",
            "an unscanned configuration is not clean",
            "Current exact evidence is the only tier that can be triaged",
            "Scheduled and Historical evidence stay visible",
            "Accepted risk is not remediation",
            "POA&M status is not verification",
            "Report-only FAIL is still FAIL",
            "Creating a POA&M does not change FAIL to PASS",
            "RA-#### identifies one operator-facing renewal chain",
            "exact source decision UUID stays the machine and audit identity",
            "match on source-id, not the RA number",
            "Missing evidence does not mean clean",
        ] {
            assert!(all.contains(point), "missing teaching point: {point}");
        }
    }

    #[test]
    fn coach_never_claims_to_perform_a_mutation() {
        for module in &SECURITY_MODULES {
            for stop in module.stops {
                let text = format!("{} {}", stop.doing, stop.gated_doing.unwrap_or(""));
                assert!(
                    !text.contains("coach will") && !text.contains("coach submits"),
                    "{} promises a coach action",
                    stop.id
                );
            }
        }
    }
}
