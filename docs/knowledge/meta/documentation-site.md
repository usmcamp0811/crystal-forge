---
type: Operator Guide
title: Build and publish the documentation site
description: Explains the static knowledge site, generated NixOS module reference, local preview, browser checks, and GitLab Pages publication.
implementation_status: implemented
tags:
  - crystal-forge
  - documentation
  - okf
  - nixos
  - gitlab-pages
---

# Build and publish the documentation site

## Responsibility and source

The `docs-site` package renders the canonical `docs/knowledge` bundle and a
generated NixOS option bundle. Do not edit generated HTML or option Markdown.
Edit knowledge Markdown or module declarations, then rebuild the site.

The site is independent of the Dioxus application. The site does not use an API,
PostgreSQL, authentication, or a running Crystal Forge server. Only the selected
Markdown bundles, referenced repository images, branding, and static assets are
published. Other repository links lead to GitLab source files on `dev`.
These source links can change when `dev` changes.

The renderer adapts the dotfiles OKF publisher without depending on the dotfiles
flake. The [renderer contract and attribution](../../../packages/docs-site/README.md)
describe its publication boundary and license. Dark and light colors use the
Crystal Forge application tokens. Search and Mermaid use local assets, not a
runtime CDN. Theme preference stays in browser local storage when available.
The site displays each concept's `implementation_status`; an absent status is
`unspecified`, not evidence that the described behavior is implemented.

## Build and preview

Run from the repository root in a dedicated task worktree:

```bash
nix build .#docs-site -L
nix run .#docs-site -- 8754
```

Open <http://127.0.0.1:8754/>. The launcher binds only to loopback. Keep it in a
persistent task-owned terminal or supervisor. Use a different free port when
another task owns `8754`. No database or application instance is required.

The launcher serves an immutable Nix build. To refresh the preview, rebuild and
restart only the task-owned launcher. A browser refresh alone does not rebuild
source changes. The static files are in `result/share/docs-site/`.

## Generated module reference

The reference evaluates declarations from every exported NixOS module with the
project's Snowfall library. It includes service, Attic metrics, STIG control,
preset, and other options declared by these modules. A coverage check fails if
an exported module has no documented declaration.

The reference contains option names, descriptions, types, and source links.
Defaults and examples are omitted without evaluating their values. Those values
can require host settings or application packages. Hidden and internal options
are omitted. STIG tracking declarations reflect the modules' default enabled
state; disabled-state conditional declarations are not included.

This is a schema reference, not an evaluated host or a deployment validation.
No NixOS system, service, or database is built. The schema evaluation disables
unmatched-definition checks because configuration results are not consumed.
Enabling a STIG preset can change runtime policy; the reference does not assert
compliance. The reference is separate from the application's upstream NixOS
option metadata catalog.

## Verification

```bash
nix build .#docs-site.tests.browser -L
nix build .#checks.x86_64-linux.okf-knowledge -L
```

The site build runs renderer and option-generation unit tests. It checks relative
HTML links and asset targets. It does not verify external URLs or fragment
anchors. Raw Markdown HTML is escaped to prevent injected scripts and references
that bypass the publication boundary.

The `docs-site` browser check uses temporary loopback servers and fresh browser
contexts. It checks search, option navigation, dark/light theme persistence,
mobile navigation, branding, and URLs under a deployment prefix. It also renders
every published Mermaid diagram in both themes at both URL layouts. Screenshots,
traces, and JSON reports are in the check output's `screenshots/` directory.
This static-site check does not replace the application's `web-ui` check.

## GitLab Pages publication

`docs-site-check` runs for merge requests and the GitLab default branch. It
builds the static browser check and site, then retains `public/` and
`docs-site-screenshots/` as artifacts. Merge request pipelines do not publish
the production Pages site.

`docs-pages` publishes the checked `public/` artifact on the default branch.
The intended project URL is <https://crystal-forge.gitlab.io/crystal-forge/>.
GitLab can redirect this URL to a unique Pages domain. Confirm the actual URL
under **Deploy > Pages** after the first successful deployment. Adding the CI
configuration does not itself prove that Pages is enabled or deployed.

The README links to the intended project URL. No custom domain is configured.
All site navigation uses relative URLs so a GitLab project prefix does not
require a different build. A failed browser check prevents publication of its
artifact. Inspect the failed job logs before retrying; do not bypass the gate.
