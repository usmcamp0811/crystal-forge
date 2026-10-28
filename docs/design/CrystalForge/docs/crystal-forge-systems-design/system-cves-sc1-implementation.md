# SC1 implementation and verification notes

Status: in progress. This note does not assert that the authoritative browser
checks passed. TASK-326.2.1 is not ready to merge.
This note records the earlier SC1 implementation and host feedback, not the
TASK-326.2.2 acceptance contract. The
[CVE/POA&M continuity design, Section 29](../../cve-poam-evidence-continuity-design-spec.md#29-acceptance-criteria)
supersedes SC1's permanent retained-artifact gate, unchanged-generation
verification, and historical/current membership equality. TASK-326.2.2 is in
progress; do not treat the older tests below as validation of that work.

## Read and write boundaries

The server selects the latest running report before it tests whether the
report is usable. It matches the reported output to exactly one derivation
within the registered flake and effective configuration. The candidate menu
does not establish uniqueness. The server then reads only that derivation's
latest completed schema-1 scan. A newer failed or queued scan does not replace
the completed source. The response separately reports the newest same-target
attempt by creation time and scan ID. Failed, pending, and in-progress attempts
cannot replace the selected completed source or grant remediation authority.
The approved attempt notice requires the attempt to start after the completed
source finished. A different scan ID alone does not prove that it is newer.
In this SC1 implementation, missing retained proof left the scan readable under
`mapped_running` without remediation context or write authority. Under the
later continuity contract, retained proof alone is not a CVE gate. The shared
`view_current_cve_authority` must select the latest consistent system state,
exactly one scoped NixOS derivation, and its newest completed schema-1 scan.
Direct triage must re-resolve that evidence under locks. An inventory GET remains
read-only. Missing, ambiguous, historical, or schema-0-only evidence cannot
authorize writes and cannot be reported as clean.

The new `binding_origin` on immutable retained generations distinguishes an
unknown pre-migration origin, CF-issued deployment, and reconciled external
activation. State and heartbeat ingestion attempt CF-bound retention before
external reconciliation. External reconciliation requires the latest consistent
generation/output report, exactly one scoped NixOS derivation, the selected
certified available schema-1 artifact completed by the report time, and a
completed schema-1 scan of that derivation. It takes the snapshot-writer lock,
never fabricates a deployment, and inserts nothing on missing or ambiguous
proof. The additive migration `0278` makes an archived same-output derivation
count toward ambiguity, even though an archived sole target stays provisional.
A background repair examines up to 16 already-observed candidates each
60 seconds; its cursor advances past unprovable or failed candidates. An
individual failed repair is logged without blocking later systems. Repeated
reports and repair passes do not rewrite retained bindings. A successful
external binding entered the SC1 exact Current triage and POA&M pipeline with
its real retained artifact baseline. Under the continuity target it supplies
optional provenance, not required CVE proof. Exact observed Current authority
is independent of activation origin and Config artifact availability. Existing
non-null baselines stay
immutable; new baselines can have a NULL retained ID with exact scan and
occurrence proof. Verification may cross revisions using strictly newer exact
Current scans. Current affected environment subjects must be a subset of active
POA&M links; historical clean or moved-out links remain audit history. Bounded
server reconciliation adds new affected subjects without overriding host
decisions. Explicit historical targets remain read-only.

The CVE target and mode are independent URL parameters. Dioxus must retain
`cve_target` and `cve_mode` in the System Detail route declaration. The view
checks system identity, selection, source, read tier, and request epoch before
it displays a page. An explicit target remains read-only even when it matches
the currently running derivation. Hardening keeps its existing selector rules.

## Design comparison

The approved reference is `components/SystemDetail.jsx`,
`components/CvesView.jsx`, and `fixtures/system-cves-sc1.js`. Compare at wide
and narrow widths in dark and light themes. The supported host feedback run
captured these combinations for mapped-running and unmapped states. The
authoritative screenshot comparison remains blocked by TASK-440.

| Difference category | SC1 observation |
| --- | --- |
| Missing sections | The production CVE revision bar and package-first card are present. The SC2 header/count work is not part of this slice. |
| Reordered sections | The revision bar remains above the vulnerability card, as in the reference. |
| Merged concepts | Current read authority and exact historical browsing use one revision bar but independent server selections; they do not share write authority. |
| Missing metadata | The read response supplies newest same-target attempt metadata independently from the completed source. The existing package-first card notice displays newer failed and queued attempts. In-progress attempts use that same approved notice location. |
| Changed interactions | An explicit generation or derivation stays selected across URL navigation. Current follows a reported running target. The host check holds an A response, records reported activation B, selects the new Current B, and confirms that late A cannot replace B. A second host check holds an unsaved triage draft through a Current continuation conflict and confirms that the approved conflict footer blocks submission without discarding fields. |
| Hierarchy differences and additions | No new section or control was added to the package-first hierarchy. A source-less state does not claim the host is clean. The server-owned read-only reason uses the existing reference notice pattern. |

The SC1 host 12ha regression started with provisional external evidence and
installed a fixture row only after its selected artifact and latest observation
passed the database trigger. It then checked inventory, triage detail, and the
dialog. Separate isolated SC1 Rust tests exercised external retention. This
does not test the later unretained exact Current case or environment POA&M
membership repair; the fixture and assertions need TASK-326.2.2 coverage.

## Verification boundary

Focused SQLx, authorization, pagination, Web UI Rust, WASM, JavaScript syntax,
formatting, and rustdoc checks are run from the task worktree. The supported
`web-ui-test` host runner is a development feedback path, not the NixOS VM
check. It runs against the isolated SC1 fixture database only when its live
port and process ownership have been verified. A host screenshot does not
replace an MR screenshot from the authoritative browser check.
The Viewer, Operator, and Admin browser contexts use a mocked role bootstrap
for presentation assertions only. Isolated database and real-API requests
independently check authorization and hidden/foreign target non-disclosure.
The held-A host regression confirms that the A response was delivered after
Current B returned the reported B target and source; it then checks that the
late A body does not restore A's findings or mutation controls. A separate
mounted-draft case keeps unsaved fields through a Current page conflict and
blocks submission from the original context.

The required `12ha-system-detail-cve-inventory-fallbacks`,
`12h-system-detail-cves-grouped-justification`, and
`28-system-hardening-tab` authoritative NixOS workflows are **BLOCKED / NOT
EXECUTED**. The shared `crystal-forge-design-targets` prerequisite fails in
four unrelated TASK-440 Config Explorer states waiting for the `TARGET`
locator. Do not change TASK-440 or report SC1 ready for review while that
prerequisite remains broken.
