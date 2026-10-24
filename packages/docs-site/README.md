# Static knowledge renderer

`build.py` renders explicit Markdown bundles. The canonical bundle is
`docs/knowledge`; the options generator supplies a second bundle. Existing
`index.md` content is preserved. Missing directory indices are synthesized in
the output only. Every page shows its `implementation_status`, or `unspecified`
when the source does not declare one. Search results include the same status.

## Crystal Forge presentation

The site uses the application tokens and typography from
`packages/web-ui/assets/app.css`: dark page `#030712`, sidebar `#0b1220`,
surfaces `#0f172a`, and brand purple `#82699b`. The light theme uses the
application's corresponding light tokens. Reading width is limited to 76ch.
The home page renders the first bundle's canonical index. The grouped sidebar
includes knowledge and options, expands the active directory, and marks the
active page. Narrow screens use an inline navigation disclosure with Escape
to close it and return focus to the control.

Dark is the default. The accessible `Light theme` toggle exposes its state with
`aria-pressed`. The site stores the choice under `cf-docs-theme` and applies it
before the stylesheet loads. If storage is blocked, the toggle still works for
the current page. Mermaid diagrams are re-rendered when the theme changes.
Without JavaScript, dark styling and the complete navigation remain available.

## Integration contract

Run with Python and `markdown-it-py`, `mdit-py-plugins`, `pygments`, and `pyyaml`
from the repository's Nix package environment:

```sh
python packages/docs-site/build.py \
  --out "$out" \
  --mermaid /path/to/bundled/mermaid.min.js \
  --repository-url https://gitlab.com/crystal-forge/crystal-forge \
  --repository-root "$PWD" \
  docs/knowledge /path/to/generated/options
```

`--revision` defaults to `dev`. Bundle directory basenames must be unique; each
becomes a site directory. A generated bundle can be outside the repository.
Pass `--repository-root` in Nix builds, where a filtered source might not contain
`.git`. Interactive builds can infer the root from a bundle's `.git` ancestor.
The repository source snapshot must also contain `docs/cf-logo-transparent.png`.
The build copies this branding image to `assets/` and references it in the header.
Include the renderer's `README.md` beside `build.py` in filtered script sources;
the build distributes its provenance/license notice with the static assets.

The output contains relative URLs and supports deployment beneath a Pages path
prefix. Search uses a bundled JavaScript index. Mermaid uses the supplied local
JavaScript file, with strict rendering enabled. Neither feature requires a CDN.
Markdown raw HTML is escaped; use Markdown image/link syntax and Mermaid fences.

Links to published Markdown become HTML links. Other repository references
become GitLab `/-/blob/dev/PATH` links, with query strings and fragments preserved.
Leading-slash Markdown paths refer to the repository root. Missing repository
paths retain a source destination; the renderer cannot prove remote existence.
Only referenced image extensions are copied, including images outside a bundle
but inside the repository. Resolved symlinks cannot escape the repository or an
explicit bundle. Scripts, arbitrary files, and unreferenced images are not copied.
References outside both boundaries fail the build.
The header branding image is an explicit additional published asset.

Each successful build checks every emitted HTML `href`, `src`, and form `action`
for existing relative file targets and deployment-prefix safety. This check does
not validate remote URLs or fragment anchors. The build refuses to replace a
nonempty directory unless the renderer's `.okf-site` marker is present. Output
must be separate from the input bundles.

Run the targeted renderer tests with the same Nix Python environment:

```sh
python -m unittest discover -s packages/docs-site -p 'test_*.py' -v
```

## Browser verification

`browser_test.py` uses Python Playwright and Chromium. It does not start a server.
The owner must serve the built output both normally and beneath a path prefix.
The same server can expose both paths. For example:

```sh
python packages/docs-site/browser_test.py \
  --base-url http://127.0.0.1:8754/site/ \
  --prefix-url http://127.0.0.1:8754/project/site/ \
  --screenshots /path/to/browser-evidence
```

The script verifies dark/light tokens, persisted theme navigation, active sidebar
links, mobile disclosure and overflow, search, the generated options reference,
and rendered Mermaid SVGs on the ecosystem architecture page. It saves desktop
and mobile screenshots for both themes, plus a Playwright trace and JSON report.
Browser errors and failed local requests cause failure. The prefix must include
at least one path component and must serve the same compiled output.

Nix check dependencies: `python3Packages.playwright`, `playwright-driver.browsers`
with `PLAYWRIGHT_BROWSERS_PATH` set to that browser derivation, and the renderer's
normal Python dependencies when the check also builds the site. Use a matching
Playwright driver/browser version. A supplied `--chromium PATH` can select a Nix
Chromium executable instead. The server remains owner-controlled. Set
`--diagram-page` to another relative page when checking a different bundle.

## Provenance and license

This bounded renderer and its CSS/JavaScript assets adapt the layout, Markdown
pipeline, relative URL rewriting, explicit image collection, output ownership,
verification, search-index, and local Mermaid patterns from:

- <https://gitlab.com/usmcamp0811/dotfiles>
- Commit `9e84b3345371dba85745dec37a74725cd8a499a7`
- `packages/scripts/okf/okf_site.py`
- `packages/scripts/okf/site/app.js` and `site/style.css`

The whole dotfiles flake and its OKF CLI are not dependencies. This adaptation
adds Crystal Forge implementation-status labels, repository fallback links, safe
referenced repository images, and a standalone build CLI. Mermaid and the Python
dependencies retain their respective upstream licenses and are supplied by Nix
separately.
The CF token values and logo come from this Crystal Forge checkout, not dotfiles.

Copyright (c) 2024 Matthew Camp. The upstream author authorized this adaptation
under Crystal Forge's GNU Affero General Public License version 3 in TASK-504.
The original dotfiles non-commercial license does not apply to this authorized
adaptation. See the repository's `LICENSE` and
<https://www.gnu.org/licenses/agpl-3.0.html>.
