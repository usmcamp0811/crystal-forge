# SC1 System Detail CVEs — Claude design review

This is a design reference, not a running Crystal Forge API. It uses deterministic
design-only data. Internal scan IDs stay in `system-cves-sc1.js` and do not appear
as new product fields. The ordinary CVE tab still uses its original design data
when the `sc1` query parameter is absent.

Serve this directory as static files and open:

`http://127.0.0.1:39997/crystal-forge.html?sc1=mapped-read-only`

The query parameter uses the existing `cf-open-system` preview event to open
**orion-db-02 → CVEs**. Replace the value with any key in
`system-cves-sc1.js`. There are no scenario controls in the product UI. For a
live in-app change without unmounting a draft, run this developer-only console
command with another listed key:

```js
window.dispatchEvent(new CustomEvent("cf-sc1-design-state", {
  detail: { key: "activated-b" },
}));
```

The full design uses the existing revision bar, the `Vulnerabilities` card,
package-first rows, and the fleet-shared `CveTriageModal`. The card's existing
empty slot now distinguishes missing results from an empty completed scan.
The bar's metadata slot carries the read restriction and its reason. No new
dashboard section, server operation, or revision action was introduced.

| SC1 | Design state in `SystemDetail.jsx` | `?sc1=` key | Allowed interaction |
| --- | --- | --- | --- |
| 01 | `CveScenarioScopeBar` Current, A results | `tracked-a`, `evaluated-b` | Inspect B explicitly; Current stays A. |
| 02 | Current with full proof or read-only result | `local-proved`, `mapped-read-only` | Normal triage only with full proof. |
| 03 | Current read-only A results, proof in bar metadata | `mapped-read-only` | Expand packages, open advisory; no triage. |
| 04 | Unmapped Current card empty state | `unmapped` | Select B via Commits; returning to Current restores Unmapped. |
| 05 | Current unresolved card empty variants | `no-report`, `ambiguous`, `invalid-report` | Inspect known revisions explicitly. |
| 06 | Known target without completed scan | `known-no-scan` | Preserve target; no scan/proof-repair action. |
| 07 | Existing source remains despite attempt | `failed-rescan`, `queued-rescan` | Inspect last completed A results. |
| 08 | Partial page with loaded rows | `continuation-error` | Retry selected page; partial count stays explicit. |
| 09 | Evaluated B versus reported B | `evaluated-b`, `activated-b` | Current follows only the reported change. |
| 10 | Stable exact generation or commit in selector | `tracked-a`, `unmapped` | Choose gen #191 or B; modes/tabs/reload/history preserve intent. |
| 11 | Read pending/error and menu-only error | `read-loading`, `read-error`, `retry-pending`, `retry-failed`, `menu-error` | Retry the selected read only; menu error keeps Current results. |
| 12 | Completed scan with zero/unknown findings | `completed-empty`, `mapped-clean`, `unknown-only` | No-finding message needs source; unknown is a finding. |
| 13 | Read-only evidence versus write action | `mapped-read-only`, `mapped-clean` | No triage button; server must still reject writes. |
| 14 | Mounted triage draft with changed Current | `draft-refresh`, then developer event `activated-b` | Draft fields persist. Apply is disabled with existing modal-footer conflict; cancel/reopen uses new evidence. |
| 15 | Designed status in existing bar/card slots | All listed keys | No extra product control or field. |

`CvesView.jsx` changes only the modal's optional, System Detail-owned blocked
submission reason. Fleet callers omit that prop and keep the existing behavior.
`HardeningTab.jsx` still uses the original `RevScopeBar` and `useRevScope`.

For repeatable rendering and interaction evidence, run
`fixtures/system-cves-sc1-check.js` in a Nix shell containing `playwright-test`,
`playwright-driver`, and `nodejs`. It captures every listed state at 1440×900
and 900×768 in dark and light themes, plus the ordinary CVE layout. It checks
explicit B browsing, reload/back navigation, read retry, draft retention,
Hardening, and fleet triage. Screenshot output is outside the design project.

The existing full-width sidebar clips the global page at 390px. This SC1
reference uses the project's 900px narrow-desktop check instead of claiming
mobile support or changing the shared page shell. The 390px shell behavior
needs a separate design decision.

Backend/API identity, RBAC, and write-rejection tests belong to the SC1
production task. This design scenario cannot prove them. Header and Scanning
count-source consolidation remains SC2 work.
