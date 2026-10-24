"""Behavioral checks for publication boundaries and deployment-safe rendering."""
import json
import tempfile
import unittest
from pathlib import Path

import build


class SiteTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.repo = self.root / "repo"
        self.docs = self.repo / "docs/knowledge"
        self.options = self.root / "generated/options"
        self.out = self.root / "site"
        self.mermaid = self.write(self.root / "mermaid.js", "window.mermaid = {};\n")
        self.write(self.repo / ".git", "gitdir: example\n")
        self.write(self.repo / "docs/cf-logo-transparent.png", "branding fixture")
        self.write(self.docs / "index.md", "# Curated entrance\n\nKeep this curated explanation.\n\n[Alpha](guide/a.md)\n")
        self.write(self.docs / "guide/a.md", """---
type: Guide
title: Alpha
description: A representative knowledge page.
implementation_status: partial
---
# Alpha

[Peer](../peer.md#details) [Source](../../../packages/default.nix#L12)
[Spaced](../../../source%20file.rs?plain=1#L4) [Missing source](../../../gone.rs)
[Root source](/flake.nix) [Options](../../../../generated/options/service.md)
![Referenced](../../images/fleet.svg) ![Internal](pic.png)

```mermaid
flowchart LR
  Server --> Agent
```
""")
        # The generated bundle is outside the checkout but explicitly supplied.
        self.write(self.options / "service.md", "---\ntitle: services.crystal-forge\nimplementation_status: implemented\n---\n# services.crystal-forge\n")
        self.write(self.docs / "peer.md", "---\nimplementation_status: proposed\n---\n# Peer\n\n## Details\n")
        self.write(self.docs / "old.md", "---\nimplementation_status: historical\n---\n# Retired design\n")
        self.write(self.repo / "docs/images/fleet.svg", "<svg xmlns='http://www.w3.org/2000/svg'/>")
        self.write(self.docs / "guide/pic.png", "png")
        self.write(self.repo / "docs/images/unreferenced.png", "private")
        self.write(self.repo / "packages/default.nix", "{}")
        self.write(self.repo / "secrets.txt", "do not publish")

    @staticmethod
    def write(path, text):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path

    def site(self, **kwargs):
        return build.Site([self.docs, self.options], "https://gitlab.example/group/project", mermaid=self.mermaid, **kwargs)

    def rendered(self, path="knowledge/guide/a.html"):
        return (self.out / path).read_text(encoding="utf-8")

    def test_curated_indices_and_missing_navigation(self):
        self.site().build(self.out)
        self.assertIn("Keep this curated explanation.", self.rendered("knowledge/index.html"))
        self.assertIn("Keep this curated explanation.", self.rendered("index.html"))
        self.assertIn('href="knowledge/guide/a.html"', self.rendered("index.html"))
        self.assertIn('href="guide/a.html"', self.rendered("knowledge/index.html"))
        self.assertTrue((self.out / "knowledge/guide/index.html").is_file())
        self.assertTrue((self.out / "options/index.html").is_file())
        self.assertFalse((self.options / "index.md").exists())

    def test_repository_links_keep_fragments_queries_and_encoding(self):
        self.site().build(self.out)
        text = self.rendered()
        base = "https://gitlab.example/group/project/-/blob/dev/"
        self.assertIn(f'href="{base}packages/default.nix#L12"', text)
        self.assertIn(f'href="{base}source%20file.rs?plain=1#L4"', text)
        self.assertIn(f'href="{base}gone.rs"', text)
        self.assertIn(f'href="{base}flake.nix"', text)
        self.assertIn('href="../peer.html#details"', text)
        self.assertIn('href="../../options/service.html"', text)

    def test_only_referenced_safe_images_are_published(self):
        self.site().build(self.out)
        self.assertTrue((self.out / "repository-images/docs/images/fleet.svg").is_file())
        self.assertTrue((self.out / "repository-images/docs/knowledge/guide/pic.png").is_file())
        self.assertFalse(list(self.out.rglob("default.nix")))
        self.assertFalse(list(self.out.rglob("secrets.txt")))
        self.assertFalse(list(self.out.rglob("unreferenced.png")))
        self.assertIn('src="../../repository-images/docs/images/fleet.svg"', self.rendered())

    def test_symlink_outside_repository_is_rejected(self):
        outside = self.write(self.root / "private.png", "private")
        (self.docs / "guide/pic.png").unlink()
        (self.docs / "guide/pic.png").symlink_to(outside)
        with self.assertRaisesRegex(ValueError, "escapes repository"):
            self.site().build(self.out)

    def test_status_labels_are_visible_and_searchable(self):
        self.site().build(self.out)
        for path, status in [("knowledge/guide/a.html", "partial"), ("knowledge/peer.html", "proposed"),
                             ("knowledge/old.html", "historical"), ("options/service.html", "implemented"),
                             ("knowledge/index.html", "unspecified")]:
            text = self.rendered(path)
            self.assertIn("Implementation status:", text)
            self.assertIn(f'data-status="{status}">{status}</span>', text)
        raw = self.rendered("assets/search-index.js")
        records = json.loads(raw.removeprefix("window.OKF_INDEX = ").strip().removesuffix(";"))
        self.assertEqual(next(p["s"] for p in records if p["t"] == "Alpha"), "partial")
        self.assertTrue(any(p["b"] == "options" for p in records))

    def test_local_mermaid_and_prefix_safe_assets(self):
        self.site().build(self.out)
        self.assertEqual(build.verify_site(self.out), [])
        text = self.rendered()
        self.assertIn('src="../../assets/mermaid.min.js"', text)
        self.assertIn('href="../../assets/style.css"', text)
        self.assertIn('action="../../search.html"', text)
        self.assertIn('class="language-mermaid"', text)
        self.assertEqual(self.rendered("assets/mermaid.min.js"), "window.mermaid = {};\n")
        notice = self.rendered("assets/renderer-attribution.txt")
        self.assertIn("Copyright (c) 2024 Matthew Camp", notice)
        self.assertIn("GNU Affero General Public License version 3", notice)
        self.assertIn("authorized this adaptation", notice)

    def test_branding_theme_and_accessible_mobile_navigation(self):
        self.site().build(self.out)
        text = self.rendered()
        self.assertIn('<html lang="en" data-theme="dark">', text)
        self.assertIn('src="../../assets/cf-logo-transparent.png"', text)
        self.assertEqual(self.rendered("assets/cf-logo-transparent.png"), "branding fixture")
        self.assertLess(text.index("assets/theme.js"), text.index("assets/style.css"))
        self.assertIn('aria-label="Light theme" aria-pressed="false"', text)
        self.assertIn('aria-controls="document-nav" aria-expanded="false"', text)
        self.assertIn('aria-label="Documents"', text)

    def test_navigation_exposes_both_bundles_and_expands_active_directory(self):
        self.site().build(self.out)
        text = self.rendered()
        self.assertIn('<h2>Knowledge</h2>', text)
        self.assertIn('<h2>NixOS options</h2>', text)
        self.assertIn('<details open><summary>guide</summary>', text)
        self.assertIn('href="a.html" aria-current="page">Alpha</a>', text)
        self.assertIn('href="../../options/service.html"', text)

    def test_verifier_rejects_missing_and_prefix_escaping_targets(self):
        self.site().build(self.out)
        self.write(self.out / "bad.html", '<a href="/index.html">root</a><img src="missing.png"><a href="../repo/.git">escape</a>')
        problems = build.verify_site(self.out)
        self.assertEqual(len(problems), 3)
        self.assertEqual(sum("prefix-unsafe" in problem for problem in problems), 2)

    def test_raw_html_and_metadata_are_escaped(self):
        self.write(self.docs / "unsafe.md", '---\ntitle: "<script>alert(1)</script>"\nimplementation_status: "<img>"\n---\n<script>alert(1)</script>\n')
        self.site().build(self.out)
        text = self.rendered("knowledge/unsafe.html")
        self.assertNotIn("<script>alert(1)</script>", text)
        self.assertIn("&lt;script&gt;", text)
        self.assertIn('data-status="&lt;img&gt;"', text)

    def test_foreign_output_is_preserved_and_owned_output_rebuilds(self):
        precious = self.write(self.out / "precious", "keep")
        with self.assertRaisesRegex(ValueError, "non-site"):
            self.site().build(self.out)
        self.assertEqual(precious.read_text(), "keep")
        precious.unlink()
        self.site().build(self.out)
        self.write(self.out / "obsolete.html", "stale")
        self.site().build(self.out)
        self.assertFalse((self.out / "obsolete.html").exists())

    def test_cli_contract_and_explicit_root_without_git(self):
        (self.repo / ".git").unlink()
        result = build.main(["--out", str(self.out), "--mermaid", str(self.mermaid),
                             "--repository-url", "https://gitlab.example/group/project",
                             "--repository-root", str(self.repo), str(self.docs), str(self.options)])
        self.assertEqual(result, 0)
        self.assertEqual(build.verify_site(self.out), [])


if __name__ == "__main__":
    unittest.main()
