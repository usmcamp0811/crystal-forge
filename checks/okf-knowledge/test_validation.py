"""Regression fixtures for OKF navigation and source-block preservation."""

from __future__ import annotations

import csv
import hashlib
import importlib.util
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).parent


def load(name: str, file: str):
    spec = importlib.util.spec_from_file_location(name, HERE / file)
    module = importlib.util.module_from_spec(spec)
    assert spec and spec.loader
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


validate = load("okf_validate", "validate.py")
coverage = load("okf_coverage", "coverage-audit.py")
diagram_scan = load("okf_diagram_scan", "diagram_scan.py")
mermaid = load("okf_render_mermaid", "render_mermaid.py")
mermaid_syntax = load("okf_validate_mermaid_syntax", "validate_mermaid_syntax.py")
inventory_script = HERE / "source_inventory.py"


class MarkdownValidationTests(unittest.TestCase):
    def test_gitlab_heading_slug_and_duplicate_rules(self):
        self.assertEqual(validate.slugify("A_B & C++ / D"), "a_b--c--d")
        self.assertEqual(
            validate.heading_slugs("## A_B\n## A_B\n## A_B"),
            {"a_b", "a_b-1", "a_b-2"},
        )

    def test_reference_images_and_html_resources_are_parsed(self):
        links = list(validate.iter_links(
            "![diagram][fig]\n\n[fig]: images/a%20b.svg#part\n"
            "<img src=\"images/x.svg\"><a href=\"other.md#section\">x</a>\n"
            "`[not](a.md)`\n```md\n[x](code.md)\n```\n"
        ))
        targets = [entry[2] for entry in links]
        self.assertEqual(targets, ["images/a%20b.svg#part", "images/x.svg", "other.md#section"])

    def test_blockquoted_html_fence_is_not_a_live_resource(self):
        links = list(validate.iter_links(
            '> ```html\n'
            '> <img src="../docs/cf-logo-transparent.png" alt="example">\n'
            '> ```\n'
        ))
        self.assertEqual(links, [])

    def test_broken_same_page_cross_file_anchor_and_traversal_are_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "docs").mkdir()
            source = root / "docs/source.md"
            (root / "docs/target.md").write_text("## Good_Heading\n")
            source.write_text("[same](#missing) [cross](target.md#missing) [escape](../../outside.md)\n")
            report = validate.Report()
            validate.check_links(report, source, Path("docs/source.md"), root, {})
            self.assertEqual(len(report.errors), 3, report.errors)

    def test_prose_reflow_keeps_punctuation_and_order(self):
        self.assertTrue(coverage.preserved("One, two; three!", ["One,\ntwo; three!"]))
        self.assertFalse(coverage.preserved("One, two; three!", ["One two; three!"]))
        self.assertFalse(coverage.preserved("first\nsecond", ["second first"]))
        self.assertFalse(coverage.preserved("same\nsame", ["same"]))

    def test_relative_link_targets_may_change_but_text_and_urls_may_not(self):
        source = "See [the guide](docs/guide.md) and [site](https://example.com/a).\n"
        self.assertTrue(coverage.preserved(source, ["See [the guide](../ops/guide.md#x) and [site](https://example.com/a).\n"]))
        self.assertFalse(coverage.preserved(source, ["See [the guide](../ops/guide.md) and [site](https://example.com/b).\n"]))
        self.assertFalse(coverage.preserved(source, ["See [the manual](../ops/guide.md) and [site](https://example.com/a).\n"]))
        self.assertFalse(coverage.preserved(source, ["See the guide and [site](https://example.com/a).\n"]))
        table = "| Doc |\n|---|\n| [a](x/a.md) |\n"
        self.assertTrue(coverage.preserved(table, ["| Doc |\n|---|\n| [a](../y/a.md) |\n"]))
        self.assertFalse(coverage.preserved(table, ["| Doc |\n|---|\n| [b](../y/a.md) |\n"]))
        code = "```sh\ncat [x](docs/a.md)\n```\n"
        self.assertFalse(coverage.preserved(code, ["```sh\ncat [x](docs/b.md)\n```\n"]))

    def test_code_indentation_and_table_rows_are_exact(self):
        source = "```sh\n  command --flag\n```\n\n| A | B |\n|---|---|\n| x | y |\n"
        self.assertTrue(coverage.preserved(source, [source]))
        self.assertFalse(coverage.preserved(source, [source.replace("  command", " command")]))
        self.assertFalse(coverage.preserved(source, [source.replace("| x | y |", "| y | x |")]))

    def test_ascii_diagram_candidate_and_literal_command_output(self):
        diagram = "```text\n+-----+     +-----+\n| A | ---> | B |\n+-----+     +-----+\n```"
        command = "```sh\nprintf 'a -> b'\n```"
        self.assertTrue(diagram_scan.scan_text("a.md", diagram))
        self.assertFalse(diagram_scan.scan_text("a.md", command))


class GitBaselineTests(unittest.TestCase):
    def setup_repo(self, root: Path, source: str, destination: str, map_row: str):
        (root / "checks/okf-knowledge").mkdir(parents=True)
        (root / "docs/knowledge/meta/migration-manifest").mkdir(parents=True)
        (root / "docs/knowledge/concept.md").write_text(destination)
        (root / "source.md").write_text(source)
        manifest = (
            "| Original | Destination | Action | Coverage |\n|---|---|---|---|\n"
            "| `source.md` | [Concept](../../concept.md) | moved | complete |\n\n"
            "## Source inventory\n### `source.md`\n"
            "| Source section | Destination |\n|---|---|\n"
            f"{map_row}\n"
        )
        (root / "docs/knowledge/meta/migration-manifest/group.md").write_text(manifest)
        (root / "checks/okf-knowledge/source-scope.tsv").write_text("# pattern\tclass\treason\n")
        subprocess.run(["git", "init", "-q"], cwd=root, check=True)
        subprocess.run(["git", "config", "user.email", "test@example.invalid"], cwd=root, check=True)
        subprocess.run(["git", "config", "user.name", "OKF test"], cwd=root, check=True)
        subprocess.run(["git", "add", "source.md"], cwd=root, check=True)
        subprocess.run(["git", "commit", "-qm", "baseline"], cwd=root, check=True)
        return subprocess.run(["git", "rev-parse", "HEAD"], cwd=root, check=True,
                              capture_output=True, text=True).stdout.strip()

    def test_split_destinations_reflow_and_exact_manifest_mapping_pass(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            src = "## Scope\nFirst, second.\n"
            self.setup_repo(root, src, "## Scope\nFirst,\nsecond.\n", "| `## Scope` | [Concept](../../concept.md#scope) |")
            # Verify only mechanics here; the CLI mapping parser is exercised
            # by the dedicated temporary-repository command test below.
            self.assertTrue(coverage.preserved(src, ["## Scope\nFirst,\nsecond.\n"]))

    def test_missing_history_fails_with_status_two(self):
        with self.assertRaises(subprocess.CalledProcessError):
            coverage.require_revision("deadbeef")

    def test_coverage_cli_accepts_mapped_block_and_rejects_lost_block(self):
        for destination, expected in (("## Scope\nFirst, second.\n", 0),
                                      ("## Scope\nFirst.\n", 1)):
            with self.subTest(expected=expected), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                base = self.setup_repo(root, "## Scope\nFirst, second.\n", destination,
                                       "| `## Scope` | [Concept](../../concept.md#scope) |")
                completed = subprocess.run(
                    [sys.executable, str(HERE / "coverage-audit.py"), "--base", base,
                     "--repo-root", str(root), "--bundle", "docs/knowledge",
                     "--scope", "checks/okf-knowledge/source-scope.tsv"],
                    cwd=root, capture_output=True, text=True,
                )
                self.assertEqual(completed.returncode, expected, completed.stdout + completed.stderr)

    def test_split_block_maps_to_two_destination_files(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = "## Scope\nAlpha, beta.\n"
            self.setup_repo(root, source, "## Scope\nAlpha,\n",
                            "| `## Scope` | [Part A](../../concept.md#scope) |")
            bundle = root / "docs/knowledge"
            (bundle / "part-b.md").write_text("## Part B\nbeta.\n")
            manifest_path = bundle / "meta/migration-manifest/group.md"
            text = manifest_path.read_text().replace(
                "[Concept](../../concept.md)",
                "[Concept](../../concept.md), [Part B](../../part-b.md)",
            ).replace(
                "[Part A](../../concept.md#scope)",
                "[Part A](../../concept.md#scope), [Part B](../../part-b.md#part-b)",
            )
            manifest_path.write_text(text)
            base = subprocess.run(["git", "rev-parse", "HEAD"], cwd=root, check=True,
                                  capture_output=True, text=True).stdout.strip()
            result = subprocess.run(
                [sys.executable, str(HERE / "coverage-audit.py"), "--base", base,
                 "--repo-root", str(root), "--bundle", "docs/knowledge"],
                cwd=root, capture_output=True, text=True,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_inventory_fails_when_baseline_document_is_omitted(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            base = self.setup_repo(root, "## Scope\nContent.\n", "## Scope\nContent.\n",
                                   "| `## Scope` | [Concept](../../concept.md#scope) |")
            (root / "omitted.md").write_text("Source material.\n")
            subprocess.run(["git", "add", "omitted.md"], cwd=root, check=True)
            subprocess.run(["git", "commit", "-qm", "add unmapped document"], cwd=root, check=True)
            baseline = subprocess.run(["git", "rev-parse", "HEAD"], cwd=root, check=True,
                                      capture_output=True, text=True).stdout.strip()
            result = subprocess.run(
                [sys.executable, str(inventory_script), "--baseline", baseline,
                 "--repo-root", str(root)], cwd=root, capture_output=True, text=True,
            )
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("omitted.md", result.stdout)

    def test_byte_identical_retained_document_needs_no_redundant_h2_map(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            base = self.setup_repo(root, "## Scope\nContent.\n", "## Scope\nContent.\n", "")
            manifest = root / "docs/knowledge/meta/migration-manifest/group.md"
            manifest.write_text(
                "| Original | Destination | Action | Coverage |\n|---|---|---|---|\n"
                "| `source.md` | [Concept](../../concept.md) | retained | complete |\n"
            )
            completed = subprocess.run(
                [sys.executable, str(HERE / "coverage-audit.py"), "--base", base,
                 "--repo-root", str(root), "--bundle", "docs/knowledge",
                 "--scope", "checks/okf-knowledge/source-scope.tsv"],
                cwd=root, capture_output=True, text=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)

    def test_source_adjustment_is_bound_to_one_exact_baseline_block(self):
        block = "## Quick Start Guide\n- Code: `/home/example/work/packages/web-ui/`\n"
        digest = hashlib.sha256(block.encode()).hexdigest()
        adjusted = coverage.apply_source_adjustment(
            "docs/design/FIGMA_CLAUDE_WORKFLOW.md",
            (digest, "C-083", "normalize-absolute-web-ui-path"),
            block,
        )
        self.assertIn("- Code: `packages/web-ui/`", adjusted)
        with self.assertRaises(ValueError):
            coverage.apply_source_adjustment(
                "docs/design/FIGMA_CLAUDE_WORKFLOW.md",
                ("0" * 64, "C-083", "normalize-absolute-web-ui-path"),
                block,
            )
        with self.assertRaises(ValueError):
            coverage.apply_source_adjustment(
                "docs/design/OTHER.md",
                (digest, "C-083", "normalize-absolute-web-ui-path"),
                block,
            )

    def test_reviewed_mermaid_conversion_preserves_surrounding_source_block(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = (
                "## Flow\n"
                "A starts the operation.\n\n"
                "```text\nA --> B\nB --> C\n```\n\n"
                "B returns the result.\n"
            )
            base = self.setup_repo(
                root,
                source,
                "## Flow\nA starts the operation.\n\n"
                "```mermaid\n%% diagram-id: conversion-flow\nflowchart LR\nA --> B\nB --> C\n```\n\n"
                "B returns the result.\n",
                "| `## Flow` | [Concept](../../concept.md#scope) |",
            )
            current_source = root / "source.md"
            current_source.write_text(
                "## Flow\nA starts the operation.\n\n"
                "```mermaid\n%% diagram-id: conversion-flow\nflowchart LR\nA --> B\nB --> C\n```\n\n"
                "B returns the result.\n"
            )
            manifest = root / "docs/knowledge/meta/migration-manifest/group.md"
            manifest.write_text(
                "| Original | Destination | Action | Coverage |\n|---|---|---|---|\n"
                "| `source.md` | [Concept](../../concept.md) | retained | complete |\n"
            )
            audit = root / "checks/okf-knowledge/diagram-audit"
            audit.mkdir(parents=True)
            with (audit / "core.tsv").open("w", encoding="utf-8", newline="") as handle:
                writer = csv.writer(handle, delimiter="\t")
                writer.writerow(["source_path", "source_heading", "purpose",
                                 "source_entities_fields", "source_relationships_order",
                                 "destination_path", "mermaid_diagram_id", "mermaid_type",
                                 "semantic_review", "render_status"])
                writer.writerow(["source.md", "Flow", "One directed step", "A; B",
                                 "A to B to C", "source.md",
                                 "conversion-flow", "flowchart",
                                 "Reviewed: preserves A to B to C", "pending"])
            result = subprocess.run(
                [sys.executable, str(HERE / "coverage-audit.py"), "--base", base,
                 "--repo-root", str(root), "--bundle", "docs/knowledge",
                 "--scope", "checks/okf-knowledge/source-scope.tsv"],
                cwd=root, capture_output=True, text=True,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


class MermaidLedgerTests(unittest.TestCase):
    def fixture(self, root: Path, relationship: str = "A points to B"):
        ledger_dir = root / "checks/okf-knowledge/diagram-audit"
        ledger_dir.mkdir(parents=True)
        docs = root / "docs/knowledge"
        docs.mkdir(parents=True)
        with (ledger_dir / "ui.tsv").open("w", encoding="utf-8", newline="") as file:
            writer = csv.writer(file, delimiter="\t")
            writer.writerow(["source_path", "source_heading", "purpose", "source_entities_fields",
                             "source_relationships_order", "destination_path", "mermaid_diagram_id",
                             "mermaid_type", "semantic_review", "render_status"])
            writer.writerow(["source.md", "Flow", "fixture", "A; B", relationship,
                             "docs/knowledge/flow.md", "fixture-flow", "flowchart",
                             "Reviewed: nodes A and B; directed edge A to B", "pending"])

    def test_valid_semantic_mapping_fixture_passes(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.fixture(root)
            (root / "docs/knowledge/flow.md").write_text(
                "```mermaid\n%% diagram-id: fixture-flow\nflowchart LR\nA --> B\n```\n")
            self.assertEqual(mermaid.check_ledger(root), [])

    def test_missing_relationship_metadata_fails(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.fixture(root, "")
            (root / "docs/knowledge/flow.md").write_text(
                "```mermaid\n%% diagram-id: fixture-flow\nflowchart LR\nA --> B\n```\n")
            errors = mermaid.check_ledger(root)
            self.assertTrue(any("source_relationships_order" in error for error in errors), errors)

    def test_diagram_exceptions_are_hash_exact_and_multiplicity_aware(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = "A -> B -> C\n"
            candidates = diagram_scan.scan_text("source.md", source)
            path, digest = diagram_scan.candidate_key(candidates[0])
            exceptions = root / "exceptions.tsv"
            exceptions.write_text(
                "path\tcontent_sha256\tcount\treason\n"
                f"{path}\t{digest}\t1\tInline prose, not a diagram.\n"
            )
            allowed = diagram_scan.load_exceptions(exceptions)
            unclassified, stale = diagram_scan.check_against_exceptions(candidates, allowed)
            self.assertEqual((unclassified, stale), ([], []))

            changed = diagram_scan.scan_text("source.md", "A -> B -> D\n")
            unclassified, stale = diagram_scan.check_against_exceptions(changed, allowed)
            self.assertEqual(len(unclassified), 1)
            self.assertEqual(len(stale), 1)

    def test_invalid_mermaid_has_expected_nonzero_parser_status(self):
        node = os.environ.get("OKF_MERMAID_NODE")
        module = os.environ.get("OKF_MERMAID_MODULE")
        if not node or not module:
            self.skipTest("pinned Mermaid parser is supplied by the locked Nix check environment")
        status, report, _diagnostic = mermaid_syntax.parse_sources(
            node,
            module,
            [{"path": "fixture.md", "index": 0, "source": "flowchart LR\nA -->"}],
        )
        self.assertEqual(status, 1)
        self.assertTrue(report["errors"])


def sha(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def anchor(heading: str) -> str:
    return validate.slugify(heading)


class AuditFixture(unittest.TestCase):
    """Builds a throwaway Git repository and runs ``coverage-audit.py`` on it.

    The baseline commit holds only source documents. Knowledge files, manifests,
    and ledgers are written after that commit, as in the real migration.
    """

    COLUMNS = (
        "source_path", "source_heading", "baseline_block_sha256", "adjustment_id",
        "destination_path", "destination_heading", "destination_block_sha256",
        "transformation", "reason",
    )

    def setUp(self):
        self._temp = tempfile.TemporaryDirectory()
        self.addCleanup(self._temp.cleanup)
        self.root = Path(self._temp.name)
        for args in (["init", "-q"], ["config", "user.email", "test@example.invalid"],
                     ["config", "user.name", "OKF test"]):
            subprocess.run(["git", *args], cwd=self.root, check=True)

    def write(self, rel: str, text: str) -> None:
        path = self.root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def commit_baseline(self, files: dict[str, str]) -> str:
        for rel, text in files.items():
            self.write(rel, text)
        subprocess.run(["git", "add", "-A"], cwd=self.root, check=True)
        subprocess.run(["git", "commit", "-qm", "baseline"], cwd=self.root, check=True)
        return subprocess.run(["git", "rev-parse", "HEAD"], cwd=self.root, check=True,
                              capture_output=True, text=True).stdout.strip()

    def write_manifest(self, entries: list[dict]) -> None:
        """Each entry: source, dest (bundle-relative), sections [(heading, dest, anchor)]."""
        cells = "".join(
            f"| `{e['source']}` | [Concept](../../{e['dest']}) | {e.get('action', 'moved')} | complete |\n"
            for e in entries
        )
        detail = ""
        for e in entries:
            detail += f"\n### `{e['source']}`\n\n| Source section | Destination |\n|---|---|\n"
            for heading, dest, frag in e.get("sections", []):
                detail += f"| `## {heading}` | [Concept](../../{dest}#{frag}) |\n"
        self.write(
            "docs/knowledge/meta/migration-manifest/test.md",
            "| Original | Destination | Action | Coverage |\n|---|---|---|---|\n"
            f"{cells}\n## Source inventory\n{detail}",
        )

    def write_support(self, cleanup_ids=("C-001",), adjustments: str = "") -> None:
        self.write("checks/okf-knowledge/source-scope.tsv", "# pattern\tclass\treason\n")
        self.write(
            "docs/knowledge/meta/cleanup-record.md",
            "# Cleanup\n\n" + "".join(f"| {i} | old | new | dest | evidence |\n" for i in cleanup_ids),
        )
        if adjustments:
            self.write("checks/okf-knowledge/source-adjustments.tsv", adjustments)

    def write_ledger(self, rows: list[dict]) -> None:
        path = self.root / "checks/okf-knowledge/semantic-replacements.tsv"
        with path.open("w", encoding="utf-8", newline="") as handle:
            writer = csv.writer(handle, delimiter="\t")
            writer.writerow(self.COLUMNS)
            writer.writerows([[row[c] for c in self.COLUMNS] for row in rows])

    def row(self, source_block: str, dest_block: str, **overrides) -> dict:
        row = {
            "source_path": "source.md", "source_heading": "Old Title",
            "baseline_block_sha256": sha(source_block), "adjustment_id": "C-001",
            "destination_path": "docs/knowledge/concept.md",
            "destination_heading": "New Title", "destination_block_sha256": sha(dest_block),
            "transformation": "semantic-replacement", "reason": "Owner-authorized rewrite.",
        }
        row.update(overrides)
        return row

    def audit(self, base: str) -> tuple[int, str, str]:
        done = subprocess.run(
            [sys.executable, str(HERE / "coverage-audit.py"), "--base", base,
             "--repo-root", str(self.root)],
            cwd=self.root, capture_output=True, text=True,
        )
        return done.returncode, done.stdout, done.stderr

    def assert_audit_fails(self, base: str, *expected: str) -> str:
        code, out, err = self.audit(base)
        self.assertEqual(code, 1, f"expected failure\nstdout:\n{out}\nstderr:\n{err}")
        self.assertNotIn("Traceback", err)
        for text in expected:
            self.assertIn(text, out)
        return out


class SemanticReplacementTests(AuditFixture):
    """Exercises ``verify_semantic_replacement`` through the real CLI."""

    SOURCE = "## Old Title\nOld content here.\n"
    DEST = "## New Title\nNew content here.\n"

    def scenario(self, *, dest=None, dest_file="concept.md", frag="new-title",
                 cleanup=("C-001",), ledger=None, **overrides) -> str:
        """Writes a one-H2 migration whose destination text differs from the source."""
        dest = self.DEST if dest is None else dest
        base = self.commit_baseline({"source.md": self.SOURCE})
        self.write("docs/knowledge/concept.md", dest)
        self.write("docs/knowledge/other.md", self.DEST)
        self.write_manifest([{
            "source": "source.md", "dest": dest_file,
            "sections": [("Old Title", dest_file, frag)],
        }])
        self.write_support(cleanup)
        self.write_ledger([self.row(self.SOURCE, self.DEST, **overrides)] if ledger is None else ledger)
        return base

    def test_valid_replacement_passes(self):
        code, out, err = self.audit(self.scenario())
        self.assertEqual(code, 0, f"{out}\n{err}")

    def test_unapproved_changed_prose_still_fails(self):
        self.assert_audit_fails(self.scenario(ledger=[]), "content/order/punctuation lost in H2 'Old Title'")

    def test_wrong_destination_path_not_in_manifest_mapping_fails(self):
        # other.md exists and has the right block hash, but the manifest maps
        # the source H2 to concept.md only.
        self.assert_audit_fails(
            self.scenario(destination_path="docs/knowledge/other.md"),
            "is not a manifest-mapped destination",
        )

    def test_wrong_destination_heading_fails(self):
        self.assert_audit_fails(
            self.scenario(dest=self.DEST + "\n## Other Title\nOther.\n",
                          destination_heading="Other Title",
                          destination_block_sha256=sha("## Other Title\nOther.\n")),
            "is not a manifest-mapped destination",
        )

    def test_duplicate_destination_heading_fails(self):
        self.assert_audit_fails(
            self.scenario(dest=self.DEST + "\n" + self.DEST), "must have exactly one H2 'New Title', found 2"
        )

    def test_wrong_source_hash_fails(self):
        self.assert_audit_fails(self.scenario(baseline_block_sha256=sha("tampered")), "baseline_block_sha256 mismatch")

    def test_wrong_destination_hash_fails(self):
        self.assert_audit_fails(self.scenario(destination_block_sha256=sha("tampered")), "destination_block_sha256 mismatch")

    def test_missing_cleanup_record_id_fails(self):
        self.assert_audit_fails(self.scenario(cleanup=("C-002",)), "cleanup-record.md has no entry C-001")

    def test_missing_destination_file_fails(self):
        # The manifest maps a file that does not exist, so mapping fails first.
        base = self.scenario(dest_file="missing.md")
        self.assert_audit_fails(base, "destination is missing")

    def test_absolute_destination_fails_cleanly(self):
        self.assert_audit_fails(
            self.scenario(destination_path=str(self.root / "docs/knowledge/concept.md")),
            "destination_path must be repository-relative",
        )

    def test_parent_traversal_destination_fails_cleanly(self):
        self.assert_audit_fails(
            self.scenario(destination_path="docs/../docs/knowledge/concept.md"), "must not contain '..'"
        )

    def test_unnormalized_destination_fails_cleanly(self):
        self.assert_audit_fails(
            self.scenario(destination_path="docs/knowledge//concept.md"), "is not normalized"
        )

    def test_duplicate_ledger_key_fails(self):
        row = self.row(self.SOURCE, self.DEST)
        self.assert_audit_fails(
            self.scenario(ledger=[row, dict(row, adjustment_id="C-002")], cleanup=("C-001", "C-002")),
            "duplicate semantic replacement key",
        )

    def test_unsupported_transformation_fails(self):
        self.assert_audit_fails(self.scenario(transformation="rewrite"), "unsupported transformation 'rewrite'")

    def test_malformed_adjustment_id_and_hash_fail(self):
        out = self.assert_audit_fails(
            self.scenario(adjustment_id="C-1", baseline_block_sha256="ABC"), "adjustment_id must match C-NNN"
        )
        self.assertIn("baseline_block_sha256 must be 64 lowercase hexadecimal", out)

    def test_empty_required_field_fails(self):
        self.assert_audit_fails(self.scenario(adjustment_id=""), "empty required fields ['adjustment_id']")

    def test_short_row_fails_without_traceback(self):
        base = self.scenario()
        path = self.root / "checks/okf-knowledge/semantic-replacements.tsv"
        lines = path.read_text().splitlines()
        path.write_text(lines[0] + "\n" + "\t".join(lines[1].split("\t")[:5]) + "\n")
        self.assert_audit_fails(base, "row is missing required fields")

    def test_missing_header_column_fails_without_traceback(self):
        base = self.scenario()
        path = self.root / "checks/okf-knowledge/semantic-replacements.tsv"
        path.write_text(path.read_text().replace("reason", "why", 1))
        self.assert_audit_fails(base, "header is missing required columns")

    def test_stale_row_fails(self):
        self.assert_audit_fails(
            self.scenario(source_heading="No Such Heading"), "stale semantic replacement C-001"
        )

    def test_row_cannot_authorize_a_different_h2_of_the_same_source(self):
        source = "## A\nAlpha text.\n\n## B\nBeta text.\n"
        base = self.commit_baseline({"source.md": source})
        self.write("docs/knowledge/concept.md", "## A\nAlpha text.\n\n## Rewritten\nNew beta.\n")
        self.write_manifest([{"source": "source.md", "dest": "concept.md", "sections": [
            ("A", "concept.md", "a"), ("B", "concept.md", "rewritten")]}])
        self.write_support()
        # The row names H2 A but carries the hash of H2 B's baseline block.
        self.write_ledger([self.row(
            "## B\nBeta text.\n", "## Rewritten\nNew beta.\n", source_heading="A",
            destination_heading="Rewritten")])
        out = self.assert_audit_fails(base, "'A': semantic replacement C-001: baseline_block_sha256 mismatch")
        self.assertIn("content/order/punctuation lost in H2 'B'", out)

    def test_row_cannot_authorize_a_different_source_file(self):
        base = self.commit_baseline({"a.md": self.SOURCE, "b.md": self.SOURCE})
        self.write("docs/knowledge/concept.md", self.DEST)
        self.write_manifest([
            {"source": "a.md", "dest": "concept.md", "sections": [("Old Title", "concept.md", "new-title")]},
            {"source": "b.md", "dest": "concept.md", "sections": [("Old Title", "concept.md", "new-title")]},
        ])
        self.write_support()
        self.write_ledger([self.row(self.SOURCE, self.DEST, source_path="a.md")])
        out = self.assert_audit_fails(base, "b.md: content/order/punctuation lost in H2 'Old Title'")
        self.assertNotIn("a.md:", out)

    def test_replacement_overlapping_source_adjustment_fails(self):
        source = "## Guide\nSee docs/frontend-component-standards.md here.\n"
        dest = "## Guide\nNew guidance.\n"
        base = self.commit_baseline({"CONTRIBUTING.md": source})
        self.write("docs/knowledge/concept.md", dest)
        self.write_manifest([{"source": "CONTRIBUTING.md", "dest": "concept.md",
                              "sections": [("Guide", "concept.md", "guide")]}])
        self.write_support(adjustments=(
            "source_path\tsource_heading\tbaseline_block_sha256\tadjustment_id\ttransformation\treason\n"
            f"CONTRIBUTING.md\tGuide\t{sha(source)}\tC-001\tnormalize-contributing-doc-path\tpath\n"))
        self.write_ledger([self.row(source, dest, source_path="CONTRIBUTING.md",
                                    source_heading="Guide", destination_heading="Guide")])
        self.assert_audit_fails(base, "overlaps a source adjustment")


class ReplaceExactTextTests(AuditFixture):
    """``replace-exact-text`` corrects one claim and still proves the rest."""

    SOURCE = "## Guide\nKeep this accurate sentence. The server wakes builders instantly.\nAlso keep this.\n"
    OLD = "The server wakes builders instantly."
    NEW = "Builders poll the server."

    def scenario(self, dest_body: str, *, old=None, new=None, digest=None, extra="", transformation="replace-exact-text"):
        base = self.commit_baseline({"source.md": self.SOURCE})
        self.write("docs/knowledge/concept.md", "## Guide\n" + dest_body)
        self.write_manifest([{"source": "source.md", "dest": "concept.md", "sections": [("Guide", "concept.md", "guide")]}])
        self.write_support()
        import json as _json
        old = self.OLD if old is None else old
        new = self.NEW if new is None else new
        self.write(
            "checks/okf-knowledge/source-adjustments.tsv",
            "source_path\tsource_heading\tbaseline_block_sha256\tadjustment_id\ttransformation\treason\told_text\tnew_text\n"
            f"source.md\tGuide\t{digest or sha(self.SOURCE.split(chr(10), 1)[0] + chr(10) + self.SOURCE.split(chr(10), 1)[1])}\tC-001\t{transformation}\tCorrection.\t"
            f"{_json.dumps(old)}\t{_json.dumps(new)}\n" + extra,
        )
        return base

    def test_corrected_claim_with_rest_preserved_passes(self):
        base = self.scenario("Keep this accurate sentence. Builders poll the server.\nAlso keep this.\n")
        code, out, err = self.audit(base)
        self.assertEqual(code, 0, f"{out}\n{err}")

    def test_rest_of_the_block_is_still_compared(self):
        base = self.scenario("Keep this accurate sentence. Builders poll the server.\n")
        self.assert_audit_fails(base, "content/order/punctuation lost in H2 'Guide'")

    def test_stale_claim_must_not_survive_as_the_only_proof(self):
        # The destination keeps the old claim instead of the correction.
        base = self.scenario("Keep this accurate sentence. The server wakes builders instantly.\nAlso keep this.\n")
        self.assert_audit_fails(base, "content/order/punctuation lost in H2 'Guide'")

    def test_claim_must_occur_exactly_once(self):
        base = self.scenario("Keep this accurate sentence. Builders poll the server.\nAlso keep this.\n", old="no such claim")
        self.assert_audit_fails(base, "expected exactly one occurrence of the corrected claim, found 0")

    def test_wrong_block_hash_fails(self):
        base = self.scenario("Keep this accurate sentence. Builders poll the server.\nAlso keep this.\n", digest=sha("x"))
        self.assert_audit_fails(base, "baseline block hash does not match")

    def test_text_columns_are_rejected_for_other_transformations(self):
        base = self.scenario("Keep this accurate sentence. Builders poll the server.\nAlso keep this.\n",
                             transformation="normalize-contributing-doc-path")
        code, out, err = self.audit(base)
        self.assertEqual(code, 2)
        self.assertIn("apply only to replace-exact-text", err)


class CleanupRecordIsNotADestinationTests(AuditFixture):
    def test_text_quoted_in_cleanup_record_does_not_satisfy_preservation(self):
        base = self.commit_baseline({"source.md": "## Old Title\nOld content here.\n"})
        self.write("docs/knowledge/concept.md", "## New Title\nNew content here.\n")
        self.write_support()
        # The old claim is quoted in the cleanup record, which the annotated
        # cell links to. The link must not count as a destination.
        self.write("docs/knowledge/meta/cleanup-record.md", "| C-001 | ## Old Title Old content here. | x | y | z |\n")
        self.write(
            "docs/knowledge/meta/migration-manifest/test.md",
            "| Original | Destination | Action | Coverage |\n|---|---|---|---|\n"
            "| `source.md` | [Concept](../../concept.md) | moved | complete |\n\n"
            "## Source inventory\n\n### `source.md`\n\n| Source section | Destination |\n|---|---|\n"
            "| `## Old Title` | [Concept](../../concept.md#new-title) (rewritten; see "
            "[cleanup record](../cleanup-record.md)) |\n",
        )
        self.assert_audit_fails(base, "content/order/punctuation lost in H2 'Old Title'")


class PreambleTests(AuditFixture):
    """The content before the first H2 is a preservation block."""

    def migrate(self, source: str, dest: str) -> str:
        base = self.commit_baseline({"source.md": source})
        self.write("docs/knowledge/concept.md", dest)
        sections = [(h, "concept.md", anchor(h)) for h, _ in coverage.split_blocks(source)]
        self.write_manifest([{"source": "source.md", "dest": "concept.md", "sections": sections}])
        self.write_support()
        return base

    def test_preserved_preamble_passes(self):
        source = "# Title\n\nCritical warning.\n\n## Body\nText.\n"
        code, out, err = self.audit(self.migrate(source, "# Concept\n\nCritical warning.\n\n## Body\nText.\n"))
        self.assertEqual(code, 0, f"{out}\n{err}")

    def test_deleted_preamble_text_fails(self):
        source = "# Title\n\nCritical warning.\n\n## Body\nText.\n"
        self.assert_audit_fails(
            self.migrate(source, "# Concept\n\n## Body\nText.\n"),
            "lost in the preamble before the first H2",
        )

    def test_document_without_h2_is_checked(self):
        source = "# Title\n\nStep one. Step two.\n"
        base = self.migrate(source, "# Concept\n\nStep one.\n")
        self.assert_audit_fails(base, "lost in the preamble before the first H2")
        self.write("docs/knowledge/concept.md", "# Concept\n\nStep one. Step two.\n")
        code, out, err = self.audit(base)
        self.assertEqual(code, 0, f"{out}\n{err}")

    def test_fenced_preamble_indentation_stays_exact(self):
        source = "# Title\n\n```yaml\nroot:\n  child: true\n```\n\n## Body\nText.\n"
        self.assert_audit_fails(
            self.migrate(source, "# Concept\n\n```yaml\nroot:\nchild: true\n```\n\n## Body\nText.\n"),
            "lost in the preamble before the first H2",
        )

    def test_repeated_preamble_line_is_multiplicity_aware(self):
        source = "# Title\n\nRepeat this.\n\nRepeat this.\n\n## Body\nText.\n"
        self.assert_audit_fails(
            self.migrate(source, "# Concept\n\nRepeat this.\n\n## Body\nText.\n"),
            "lost in the preamble before the first H2",
        )

    def test_title_only_front_matter_only_and_empty_preambles_need_no_mapping(self):
        for source in ("# Title\n\n## Body\nText.\n", "---\nid: doc-1\n---\n\n## Body\nText.\n", "## Body\nText.\n"):
            self.assertEqual(coverage.preamble_content(source), "", source)

    def test_h3_and_prose_before_first_h2_count_as_content(self):
        self.assertIn("Warning", coverage.preamble_content("# T\n\n### Note\nWarning.\n\n## B\nx\n"))
        self.assertIn("```", coverage.preamble_content("# T\n```sh\n# not a title\n```\n## B\n"))
        self.assertIn("# not a title", coverage.preamble_content("# T\n```sh\n# not a title\n```\n## B\n"))

    def test_reserved_preamble_heading_is_rejected(self):
        with self.assertRaises(ValueError):
            coverage.split_document("## __preamble__\nx\n")


if __name__ == "__main__":
    unittest.main()
