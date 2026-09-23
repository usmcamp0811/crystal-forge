# Systems design and System Detail CVEs handoff

The complete `systems-view-design-v0.1.md` file now contains document version **0.2**.
The filename is retained so existing repository links keep working.

Start with the decision record in Section 22, then read `system-cves-chunk-1.md`.
The latter is the bounded first implementation slice. The agent prompt and manual
validation guide are in this directory. Do not ask one agent to implement the
entire Systems, fleet CVEs, or Compliance audit.

Original application audit: `58006084aa699b84bcb1d02d6f911d4d4ee94ea3`.
Decision-update branch head: `327d03b6d58055eb688fe657f12e223b8419f446`.
The owner's decision update is dated 2026-09-23.

## Main changes

Unmapped Current stays unmapped. It does not select flake head automatically.
A uniquely mapped running derivation can display its schema-1 scan read-only
when strict deployment proof is missing. Current follows observed deployments
on refresh. Explicit revisions remain exact. New evaluated revisions only
change browsing choices until deployed.

The later continuity direction keeps one stable system/CVE/package finding and
its open POA&M across revisions. It preserves the opened-against baseline.
Candidate remediation, Awaiting verification, and formal Completed are separate
concepts. Local agent scanning and automatic formal closure are not SC1 work.

## Contents and evidence

The full document retains the original screen audit, gaps, source references,
and screenshot crops. Its 12 Mermaid blocks match the separate files in
`diagrams/`. Historical diagram filenames are retained for link stability.
The full regression matrix has 36 scenarios; the SC1 contract identifies its
own smaller acceptance set.

This is a document handoff, not a code implementation or merge approval.
No repository write, database operation, application test, or browser workflow
was performed for this update. The manifest records artifact checks separately.
The existing fleet and Compliance audit documents are not overwritten by this
bundle. Section 22 limits the scope of precedence over their older proposals.
