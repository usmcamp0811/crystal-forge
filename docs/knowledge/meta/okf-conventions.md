---
type: Reference
title: Crystal Forge OKF conventions and migration note
description: Records the pinned Open Knowledge Format revision, the Crystal Forge type taxonomy, extension fields, link and provenance conventions, and the rules for maintaining this knowledge bundle.
tags:
  - crystal-forge
  - documentation
  - okf
  - conventions
---

# Crystal Forge OKF conventions and migration note

This document is the authority for how the Crystal Forge knowledge bundle in
`docs/knowledge/` is written and maintained. It does not copy the Open
Knowledge Format (OKF) specification. Read the pinned specification revision
below for the format itself.

## Pinned specification

| Field | Value |
| --- | --- |
| Format | Open Knowledge Format |
| Version | 0.2 (`okf_version: "0.2"` in [the bundle root index](../index.md)) |
| Upstream repository | <https://github.com/GoogleCloudPlatform/open-knowledge-format> |
| Upstream commit read (repository `HEAD` on the migration date) | `ad30107c31c06aec8a7d5636e0d1058118604e6f` |
| Last upstream commit that changed `SPEC.md` | `0b87c52c6ef999286c745e19998fdfcd03d5dbee` |
| `SPEC.md` blob | `c06e3eede0c910d0ecf12524c34204156f8795ac` |
| Migration date | 2026-10-03 (timestamps use the `-05:00` offset) |
| Crystal Forge base commit of the migrated sources | `3b23d36f24fbf9ba05045c43b5325fd000dbd9b2` (`origin/dev`) |

The whole migration followed this revision. Do not change the pinned revision
silently. To adopt a newer OKF revision, record the new commit here, re-run
the validator, and add a `log.md` entry.

## Bundle layout

The bundle root is `docs/knowledge/`. Every Markdown file below the root is a
concept document except the reserved names `index.md` and `log.md`.

Group directories (each has an `index.md`):

| Directory | Contents |
| --- | --- |
| `overview/` | Problem statement, constraints, context, roadmap, and product vision. |
| `architecture/` | System-wide architecture, data flows, sequences, and lifecycles that span components. |
| `components/` | The server, agent, builder, evaluator, web UI, and other deployable parts. |
| `concepts/` | Domain vocabulary that several components share. |
| `workflows/` | End-to-end processes performed by Crystal Forge itself. |
| `deployment/` | Deployment policies, policy checks, agent deployment, and system current state. |
| `evaluation/` | Evaluation, flake snapshots, Config Explorer, and NixOS option metadata. |
| `builders/` | Builder architecture, builder API, and builder security. |
| `caches/` | Binary cache destinations and cache workflows. |
| `compliance/` | Compliance bundles, policies, STIG modules, and interchange. |
| `cves/` | CVE scanning, evidence, and triage. |
| `poam/` | POA&M behavior and evidence continuity. |
| `security/` | Authentication, sessions, role mapping, and security architecture. |
| `data-model/` | Database entities, views, and query patterns. |
| `api/` | HTTP API contracts. |
| `ui/` | Web UI views, design system, coding standards, and design handoffs. |
| `operations/` | Operator and developer guides, runbooks, and development environments. |
| `testing/` | Test plans, check runbooks, and fixtures. |
| `decisions/` | Architecture decision records and recorded design decisions. |
| `references/` | External material represented as concepts. |
| `historical/` | Superseded or task-specific documents kept for their reasoning. |
| `meta/` | This note and the migration manifest. |

A new concept goes in the directory that owns its main question. Do not add a
directory for a single concept. Keep the hierarchy to one level below the
bundle root, with at most one further level for large groups.

## Type taxonomy

The `type` field comes from this controlled list. Add a type only when no
existing type fits, and add it to this table in the same change.

| `type` | Use for |
| --- | --- |
| `Architecture` | Structure and data flow that cross several components. |
| `Component` | One deployable or major part of the system. |
| `Concept` | One domain idea, term, or state meaning. |
| `Workflow` | A process that Crystal Forge executes, with steps and states. |
| `Design Specification` | Approved or proposed design intent, including partly implemented designs. |
| `Decision` | A recorded decision with context and consequences. |
| `Operator Guide` | Instructions for people who configure or operate Crystal Forge. |
| `Runbook` | Step-by-step procedure for a recurring or incident task. |
| `API` | An HTTP or protocol contract. |
| `Data Model` | Tables, views, columns, and persistence rules. |
| `Security Model` | Trust boundaries, authentication, authorization, and threats. |
| `Compliance Model` | Compliance bundles, policies, evidence, and interchange semantics. |
| `UI Design` | Views, interaction design, design systems, and UI standards. |
| `Testing Guide` | How tests and checks are structured, run, and extended. |
| `Reference` | Lookup material, external references, and this bundle's own meta documents. |
| `Historical Reference` | Superseded designs and task-bound records kept for reasoning. |

## Frontmatter

```yaml
---
type: Component
title: Crystal Forge Builder
description: One sentence that lets an agent decide whether to open the file.
tags:
  - crystal-forge
  - builder
implementation_status: implemented
status: deprecated          # only when the OKF lifecycle value is not stable
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:46:40-05:00
verified:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T23:30:00-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/builder.md at commit 3b23d36f"
    title: Original document title
---
```

Rules:

- `type`, `title`, and `description` are present on every concept. The
  description is one sentence.
- `resource` is used only when the concept describes one asset with a real
  canonical location. Do not invent URIs.
- `tags` always include `crystal-forge`.
- Standard OKF fields keep their OKF meaning. In particular, `status` is the
  OKF lifecycle (`draft`, `stable`, `deprecated`). Crystal Forge uses
  `status: deprecated` only for documents that no longer describe current
  behavior and are kept for history. It never carries implementation state.
- `generated` is present only when the migration (or a later agent) wrote or
  materially restructured the content. A concept that is a `git mv` of a
  human-authored file with frontmatter added carries no `generated` field.
- `verified` is present only when someone compared the whole behavioral
  content of the concept with the implementation or other evidence. The
  actor is a real actor: `human:<id>` only for a person who confirmed it.
  Agent verification uses the `<producer>/<version>` form and gives the
  machine-confirmed trust tier. Partial checks are recorded in the body
  section `Migration verification notes`, not in `verified`.
- Never record an author, reviewer, timestamp, URL, or trust level that no
  evidence supports. Human-authored source text does not make a concept
  `human:`-verified.
- Every timestamp is ISO 8601 with an explicit offset.

### Extension field: `implementation_status`

`implementation_status` is a Crystal Forge extension. It states whether the
described behavior exists in the current implementation. It is present on
every concept outside `references/` and `meta/` except concepts of type `Reference`.

| Value | Meaning |
| --- | --- |
| `implemented` | The described behavior exists in the current code. |
| `partial` | Part of the described behavior exists. The concept states which part. |
| `proposed` | The text describes approved or proposed intent that the code does not yet implement. |
| `historical` | The text describes behavior or reasoning that no longer applies. Pair it with `status: deprecated`. |

A concept that mixes implemented and proposed behavior is split when the
parts are independent. Otherwise it uses `partial` and marks each part with a
`Status:` line. An approved design is never rewritten to match an incomplete
implementation. Record the gap instead.

## Provenance convention

`sources` records the pre-migration document. The `resource` string is a
scope descriptor, not a URI, in this exact shape:

```text
Crystal Forge repository file <repository-relative path> at commit <short sha>
```

The same shape records implementation files used during verification. The
validator checks the shape. Use the short SHA of the commit that the file was
read at. Old paths no longer exist after a `git mv`; the commit identifies
them. Footnotes keyed to a `sources[].id` attribute individual claims when a
concept merges several sources.

## Links

- Use relative Markdown links so the bundle renders in Git hosting and local
  viewers. This is a Crystal Forge choice. OKF also permits the
  bundle-absolute `/path.md` form, but that form resolves to the repository
  root on Git hosting.
- Link to a concept once per section where the reference matters. Do not link
  every repeated term.
- Never link to a file that does not exist. The validator resolves every
  relative link, image, and diagram path.
- Do not write absolute filesystem paths in any concept.

## Documentation diagrams

Every authored visual diagram in repository documentation MUST use Mermaid.
This includes flow and sequence diagrams, state transitions, entity
relationships, component/file containment trees, timelines, and UI wireframes.
Do not keep an ASCII or Unicode duplicate beside a Mermaid diagram, hide one in
a collapsible block, or move one to an appendix. A converted diagram MUST keep
an adjacent field/behavior table when a structure diagram cannot carry all
source detail legibly. A Mermaid flowchart communicates structure or flow; it
does not establish pixel-level UI parity.

Classify a fenced or unfenced text block as literal source material rather
than a diagram only when the text itself is code, configuration, SQL, a
regular expression, a Markdown table, a test fixture, or literal command
output. Record each exception by exact path and block fingerprint in the
diagram audit ledger. Do not suppress a whole file, directory, or glob. A
directory tree that documents project structure is a diagram and MUST become
a Mermaid flowchart with containment edges and original annotations retained.

Each diagram replacement MUST have one row in
`checks/okf-knowledge/diagram-audit/*.tsv`. Record its immutable source path
and heading, purpose, entities/fields, relationships/branches/order, Mermaid
destination and diagram ID, diagram type, semantic comparison, and render
result. The blocking `okf-knowledge` Nix check validates Mermaid syntax with
the pinned Mermaid parser without launching Chromium. Include source-block and
Mermaid-block SHA-256 fingerprints. A parser or renderer pass verifies syntax
only; it does not verify semantic equivalence.

For a full SVG review, use the pinned Mermaid CLI outside the Nix build sandbox:

```sh
nix run .#okf-mermaid-renderer -- --out /tmp/cf-okf-mermaid --jobs 1
```

This manual render covers every Mermaid block in the repository. It is not a
substitute for GitLab's renderer, so keep any unavailable GitLab-preview check
marked pending. Do not send repository diagrams to an external rendering
service.

## Index files

- Each group directory has an `index.md` with sections that group concepts
  by purpose. Each entry is `* [Title](file.md) - description`, and the
  description says when to open the file.
- Index files carry no frontmatter. Only the bundle-root `index.md` carries
  `okf_version`.
- The bundle-root `index.md` links to every group index.
- Every concept is listed in exactly one group index and may also be linked
  from related concepts.

## Lossless migration rules

These rules govern source migration and structural reorganization. The
repository owner may separately authorize a semantic cleanup after migration;
see [the cleanup record](cleanup-record.md). Do not treat a cleanup as
lossless migration or hide it in a preservation exception.

1. Every substantive section of a source document has a destination, recorded
   in [the migration manifest](migration-manifest/index.md).
2. Split a document only when it mixes independent concepts. Move text
   verbatim. Allowed edits are frontmatter, heading levels, link targets, and
   status notes. A rewrite needs a recorded reason. A later semantic rewrite
   requires owner authorization and a cleanup-record entry with the old claim,
   the correction, the destination, and code or design evidence.
3. Use `git mv` when one source maps to one destination so Git history
   follows the file.
4. Reconcile disagreement between sources. Do not choose silently. State the
   disagreement and which source matches the code.
5. Proposed and historical content stays and is labeled with
   `implementation_status`.
6. A document deletion is acceptable only after the manifest shows
   `Coverage: complete` and a line-level coverage check found no lost content.
   This rule does not authorize deleting runtime code, NixOS options, SQL views,
   or migrations during a documentation cleanup.

7. A preservation failure caused by an authorized semantic cleanup is not
   waived by a blanket exception. Record the changed source claim in the
   cleanup record and update the source-block mapping to point to the corrected
   concept. Keep unrelated source blocks subject to exact preservation.

## Content kept outside the bundle

These files stay at their original paths. The reason is stable paths, not
missing documentation. The bundle indexes them.

| Path | Reason |
| --- | --- |
| `AGENTS.md`, `CLAUDE.md`, `docs/agents/` | Operational agent-control files. They are not knowledge concepts. |
| `docs/design/CrystalForge/` | Design handoff tree. Nix packages, fixture seeding, and checks read it by path. Handoff manifests carry checksums of its Markdown files. Later design handoffs overwrite it in place. |
| `backlog/` (including `backlog/docs/`) | Backlog.md manages these documents by ID. Tasks reference their paths. |
| `checks/*/README.md`, `packages/*/README.md` | README files that serve the check or package as its ecosystem entry point. |
| `README.md`, `CONTRIBUTING.md`, `CLA.md`, `.gitlab/` | Repository entry points, contribution policy, legal text, and templates. |
| `packages/slides/` | Presentation source. |
| Images under `docs/` and `docs/screenshots/` | Assets referenced by `README.md` and by the screenshot checks. |

Each retained file that holds project knowledge has a first-class concept or
catalog entry in the bundle with a description and implementation status.

## Maintenance rules

- Change behavior and documentation in the same change. A stale concept is a
  defect.
- Update `implementation_status` when a proposed or partial design ships.
- Add an entry to [the update log](../log.md) for each structural change.
- Run the `okf-knowledge` flake check before review. It enforces
  frontmatter, taxonomy, reserved names, the version declaration, links,
  asset paths, manifest completeness, the diagram ledger, Mermaid rendering,
  and the absence of absolute paths. The CI preservation job compares source
  blocks against the immutable MR merge-base after fetching the required Git
  object; the pure Nix build does not depend on Git history or the network.
- Do not add a concept without adding it to a group index.
- Do not copy the OKF specification into this repository.
