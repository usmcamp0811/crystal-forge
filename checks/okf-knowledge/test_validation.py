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


if __name__ == "__main__":
    unittest.main()
