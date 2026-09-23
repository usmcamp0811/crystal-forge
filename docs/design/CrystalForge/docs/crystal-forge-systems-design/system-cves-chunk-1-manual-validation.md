# SC1 manual validation

Use the task-owned preview, not the persistent or production database. The agent
must supply a verified URL and the exact system IDs/routes for the fixtures below.
SC1 is a read/default/navigation change. Do not expect header/Scanning totals or
cross-generation POA&M closure to be fixed by this slice.

## Handoff prerequisites

The agent must identify the application SHA, backend/UI freshness, database data
mode, task/worktree, and required-check status. The preview must use API-produced
rows for the mapped-missing-proof and unmapped examples. Browser-only fixed JSON
is not enough. No requested preview or API URL should contain secrets.

Required fixture roles: a known Current target A with scan SA; a newer evaluated
and scanned but undeployed target B with scan SB; a mapped local activation; a
mapped A without retained proof; an unmapped output; a mapped target with no scan;
and a historical/explicit target. Some cases may use separate systems to keep
validation deterministic. Include a multi-page source for continuation testing.

## 1. Confirm the default does not drift to head

Open the system that runs A. Open CVEs. Read its generation/commit and scan source.
The source must be SA. B must be available only as an explicit browsing choice.
Refresh while B is still undeployed. Current must remain A.

Pass: the selected-target label and exact scan agree with the fixture API/records.
Fail: B appears by default because it is newer, evaluated, or scanned.

## 2. Confirm local activation is not penalized by origin

Open the mapped local-activation fixture. The target must be the reconciled
running configuration. With full proof, use normal Current behavior. Without
complete proof, use the next test's read-only behavior. Do not show flake head
or call a known output unmapped solely because activation happened outside CF.

## 3. Read a mapped scan with missing deployment proof

Open the fixture with uniquely mapped A, completed SA, and no retained binding.
The package list and findings must appear. Inspect source UUID, time, scanner,
and target. The notice must name the missing proof and read-only restriction.
No Triage or other remediation write control may be enabled for this evidence.

The agent must also show an automated direct-API negative test for the mutation
boundary. Disabled UI alone is insufficient. Check the case with zero findings:
it must identify a completed empty scan, not claim full proof or permanent safety.

## 4. Keep unmapped output unmapped

Open the unmapped fixture while B has an available scan. Current must say Unmapped
and have no Current inventory. No head scan or local scan must start automatically.
Explicitly select B. Its own results may appear, clearly as B and read-only.
Return to Current. It must still be unmapped.

## 5. Distinguish no scan from no findings and a failed read

Open a known target without an eligible scan. It must retain target identity and
say no completed scan for that target. It must not show another revision's rows.
Open an eligible completed empty scan: its source/time must be visible. Open an
Unknown-only source: it must show findings, not clean. Ask the agent to demonstrate
a controlled read failure in the isolated preview: an error/retry must replace
any inference that the target is unmapped or has zero findings.

## 6. Validate refresh versus exact selection

Start in Current on A. Have the agent simulate/report B activation in the isolated
fixture using the supported test workflow. Refresh. Current must identify B and
select SB, or show B's no-scan state when appropriate. It must not mix A rows with
B's label. Newly evaluated targets that are not activated must only update choices.

Select exact A. Refresh, switch tabs, reload, then use Back/Forward. The explicit
selection must remain A. Toggle Generations/Commits. A missing menu equivalent
must not switch the data to another target. Return explicitly to Current and
confirm that it now follows B.

## 7. Check pages and late responses

Load more than one page from the read-only mapped source. Check full totals,
loaded counts and source identity. Demonstrate a continuation failure: existing
rows stay visible with retry. Change system/target during a controlled slow
response. No old-system/target rows or action state may appear in the new view.
A new scan/source during continuation must restart rather than merge sources.

## 8. Protect a draft without pretending continuity is implemented

Open a triage editor on a normal fully authoritative fixture. Enter unsaved text.
Trigger an in-app refresh or controlled new observation. The draft must not be
silently erased or rebound to another system, target, or environment.

SC1 retains the current server conflict policy. A deployment-racing submission
may still require refresh/retry until the later continuity slice. The agent must
not bypass that protection or claim that a plan now saves against captured A
and automatically continues on B. Do not test automatic closure as SC1 acceptance.

## 9. Check design and permissions

Check wide/narrow layouts and light/dark modes. Confirm revision bar, package
headers, columns, advisory links, read-only notices and keyboard disclosure.
Check as Viewer and a mutating role. The new read-only tier must remain read-only
for both. Have the agent show negative tests for a hidden system and foreign-flake
target. A shared-helper change also needs an affected Hardening regression.

## Approval record

Record Pass, Fail, or Not verified for each scenario. Attach the exact tested SHA
and preview data mode. A working first page does not approve the whole Systems
view. A blocked required browser/database check remains a blocker. Stop after
SC1 validation; SC2 handles running-header and cross-screen count consistency.
