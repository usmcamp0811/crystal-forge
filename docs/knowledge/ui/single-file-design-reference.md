---
type: Operator Guide
title: Single-file Crystal Forge design reference
description: Import a Claude HTML export into the readable canonical design reference and inspect meaningful source diffs without running a backend.
tags:
  - crystal-forge
  - design
  - workflow
implementation_status: implemented
---

# Single-file Crystal Forge design reference

## Canonical artifact

The executable design reference is
[crystal-forge.html](../../design/CrystalForge/crystal-forge.html).
It contains readable inline JavaScript, JSX, fixtures and CSS. Do not edit a
parallel component tree. The split runtime sources are superseded by this file.
React, ReactDOM, Babel and JSZip retain their exported CDN URLs, so the artifact
is single-file source, not an offline bundle of third-party libraries.

Written design contracts, review material, screenshots, diagrams, JSON seed
fixtures and the SC1 browser helper remain separate because other documentation,
tests and developer tooling consume them. They are not a second executable UI.
Do not delete independent test fixtures while replacing an export.

The user approved a fixture-only static preview for this design-reference work.
This exception does not change the isolated Dioxus preview requirement for
production application changes.

## Import an export

Run from the repository root, replacing the first path with the downloaded file:

```bash
nix shell --inputs-from . nixpkgs#python3 -c python3 \
  packages/devScripts/web-ui/design-parity/import-design-export.py \
  /tmp/opencode/claude-export.html \
  docs/design/CrystalForge/crystal-forge.html
git diff -- docs/design/CrystalForge/crystal-forge.html
```

The importer decodes gzip/base64 manifest resources without executing them,
inlines scripts in template order, preserves Babel script attributes and CSS,
and uses stable ordinal markers instead of random bundle UUIDs. Reimporting the
same export produces identical bytes. Already-normalized standalone HTML is
accepted idempotently. Unsupported nested pages, resource shapes and unresolved
local scripts fail rather than silently dropping content.

Review the generated JavaScript as untrusted source before opening it. Never use
real credentials in a design mockup. The importer is not a security sandbox.

## Preview and checks

The existing static server is available through Nix:

```bash
nix run .#design-example-server -- --port 8526
```

Its default server binding is not loopback-only. For a private loopback preview,
use this command instead, from the repository root:

```bash
nix shell --inputs-from . nixpkgs#python3 -c python3 -m http.server \
  8526 --bind 127.0.0.1 --directory docs/design/CrystalForge
```

Open `http://127.0.0.1:8526/crystal-forge.html`. In the 2026-11-16 export,
open Profile by selecting **Mira Reyes** in the sidebar; `?view=profile` does not
select that view. Use the UI rather than assuming query parameters work.

```bash
nix shell --inputs-from . nixpkgs#python3 -c python3 -B -m unittest discover \
  -s packages/devScripts/web-ui/design-parity \
  -p test_import_design_export.py -v
nix shell --inputs-from . nixpkgs#nodejs -c node \
  packages/devScripts/web-ui/design-parity/generate-design-targets-test.js
nix build .#okf-knowledge -L
```

## Agent-access handoff and known gaps

The imported export adds Profile Access Tokens adjacent to Active sessions.
It includes owner-persona previews, create, reveal, details, revoke and rotate
interactions. Existing components and CSS were unchanged apart from the Profile
insertion. The source export SHA-256 is
`c1dec2361dd37c4a37bc4736d152442b15904006f69aa9083c3e2abde25585cb`.

The export does **not** implement Admin token oversight, Access Inspector,
Server MCP status, Connect agent entry points or new remediation-context panels.
These remain requirements in
[CF-AGENT-ACCESS-001 v1.1](../security/agent-access-tokens-mcp-remediation-context.md)
and TASK-529, not functionality supplied by this export. TASK-529.1 tracks the
design handoff.

The mockup uses local React state and simulated delays, not token APIs or MCP.
Its random-secret fallback, fixed rotation expiry, partial effective-access
calculation and form limits are not production authorization contracts.
No mock token authenticates against Crystal Forge. The specification overrides
mockup behavior where they differ. A browser smoke of mock issuance is not
proof of role/environment isolation, CSRF protection or one-time server secrets.

The screenshot generator still consumes external golden JSON separately. Its
custom-fixture overlay does not replace fixtures now inline in the HTML; that
capture path needs explicit verification before approving new visual baselines.
