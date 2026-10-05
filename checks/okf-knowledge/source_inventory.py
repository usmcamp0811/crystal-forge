#!/usr/bin/env python3
"""Cross-checks the migration manifest against an immutable Git baseline.

Unlike the legacy validator, this command enumerates source files from
``git ls-tree <baseline>``. A deleted file therefore cannot disappear from the
source inventory merely because it no longer exists in the current checkout.

Every baseline Markdown-family file must have exactly one migration-manifest
row or match a narrow, documented out-of-scope category in
``source-scope.tsv``. The output inventory includes the baseline blob id and
the exact action and destination recorded by the manifest. It is intended to
run in CI after fetching the target branch; it is not part of a pure Nix build.

Usage::

    source_inventory.py --baseline <merge-base> [--write inventory.tsv]

The command exits nonzero if a baseline document is unmapped, mapped more
than once, or has an invalid disposition. It does not infer that a manifest
row proves content preservation; ``coverage-audit.py`` checks source blocks.
"""

from __future__ import annotations

import argparse
import fnmatch
import re
import subprocess
import sys
from pathlib import Path
from urllib.parse import unquote

DOC_SUFFIXES = {".md", ".mdx", ".rst", ".adoc", ".asciidoc"}
ASSET_SUFFIXES = {".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp", ".pdf", ".mmd", ".puml", ".drawio", ".json", ".txt"}
MANIFEST = Path("docs/knowledge/meta/migration-manifest")
SCOPE = Path("checks/okf-knowledge/source-scope.tsv")


def git(*args: str) -> str:
    return subprocess.run(["git", *args], check=True, capture_output=True, text=True).stdout


def baseline_tree(rev: str) -> dict[str, str]:
    # Do not turn a missing merge-base into an empty source inventory.
    subprocess.run(["git", "cat-file", "-e", f"{rev}^{{commit}}"], check=True,
                   capture_output=True)
    out = subprocess.run(["git", "ls-tree", "-r", "-z", rev], check=True, capture_output=True).stdout
    entries: dict[str, str] = {}
    for row in out.split(b"\0"):
        if not row:
            continue
        metadata, raw_path = row.split(b"\t", 1)
        fields = metadata.decode().split()
        entries[raw_path.decode("utf-8", "surrogateescape")] = fields[2]
    return entries


def manifest_rows(bundle: Path) -> dict[str, tuple[str, str]]:
    """Returns source path -> (action, destination cell), rejecting duplicates."""
    rows: dict[str, tuple[str, str]] = {}
    table_start = re.compile(r"^\|\s*Original\s*\|\s*Destination\s*\|\s*Action\s*\|\s*Coverage\s*\|")
    for path in sorted(bundle.glob("*.md")):
        if path.name in {"index.md", "log.md"}:
            continue
        in_table = False
        for line_no, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            if table_start.match(line):
                in_table = True
                continue
            if not in_table:
                continue
            if not line.startswith("|"):
                in_table = False
                continue
            if re.match(r"^\|[\s:|-]+\|$", line):
                continue
            cells = [cell.strip() for cell in re.split(r"(?<!\\)\|", line.strip().strip("|"))]
            if len(cells) != 4:
                raise ValueError(f"{path}:{line_no}: malformed manifest table row")
            source, destination, action, coverage = cells
            source = source.strip("`")
            if source in rows:
                raise ValueError(f"duplicate source {source!r} in migration manifests")
            if coverage != "complete":
                raise ValueError(f"{path}:{line_no}: {source!r} does not have Coverage: complete")
            rows[source] = (action, destination)
    return rows


def scope_rules(path: Path) -> list[tuple[str, str, str]]:
    rows = []
    for line_no, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line or line.startswith("#"):
            continue
        cells = line.split("\t")
        if len(cells) != 3 or not all(cells):
            raise ValueError(f"{path}:{line_no}: expected pattern, class, reason")
        rows.append((cells[0], cells[1], cells[2]))
    return rows


def current_paths() -> set[str]:
    return set(git("ls-files", "-z").split("\0")) - {""}


def destination_paths(cell: str, manifest_file: Path) -> list[Path]:
    targets = re.findall(r"\]\(([^)\s]+)\)", cell)
    resolved: list[Path] = []
    for target in targets:
        if re.match(r"^[A-Za-z][A-Za-z0-9+.-]*:", target):
            continue
        resolved.append((manifest_file.parent / target.partition("#")[0]).resolve())
    return resolved


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", required=True, help="immutable Git commit containing the original documentation")
    parser.add_argument("--repo-root", default=".")
    parser.add_argument("--write", help="write the verified inventory TSV to this path")
    args = parser.parse_args()

    root = Path(args.repo_root).resolve()
    inventory_dir = root / MANIFEST
    scope_file = root / SCOPE
    tree = baseline_tree(args.baseline)
    tracked_now = current_paths()
    rows = manifest_rows(inventory_dir)
    rules = scope_rules(scope_file)
    inventory: list[tuple[str, str, str, str, str]] = []
    errors: list[str] = []

    baseline_docs = {
        path: blob for path, blob in tree.items()
        if Path(path).suffix.lower() in DOC_SUFFIXES
    }
    baseline_assets = {
        path: blob for path, blob in tree.items()
        if Path(path).suffix.lower() in ASSET_SUFFIXES
    }

    # Current authored Markdown files also need source provenance. New concepts
    # resolve provenance through their sources[].resource descriptor; newly
    # authored indexes and manifests resolve through ordinary parsed links.
    current_docs = {
        path for path in tracked_now
        if Path(path).suffix.lower() in DOC_SUFFIXES
        and not path.startswith((".git/", "node_modules/", "target/", "result/"))
    }

    for source, blob in sorted(baseline_docs.items()):
        if source in rows:
            action, dest_cell = rows.pop(source)
            if action not in {"moved", "split", "merged", "retained", "replaced", "excluded"}:
                errors.append(f"{source}: invalid migration action {action!r}")
            matching_scope = [rule for rule in rules if fnmatch.fnmatchcase(source, rule[0])]
            if action == "excluded" and len(matching_scope) != 1:
                errors.append(f"{source}: exclusion must match exactly one path-specific source-scope rule")
            if action != "excluded" and matching_scope:
                errors.append(f"{source}: manifest action {action!r} conflicts with source-scope exclusion {matching_scope[0][0]}")
            manifest_path = next(
                (p for p in inventory_dir.glob("*.md") if f"`{source}`" in p.read_text(encoding="utf-8")),
                None,
            )
            if manifest_path is None:
                errors.append(f"{source}: manifest row has no owning manifest file")
                continue
            targets = destination_paths(dest_cell, manifest_path)
            if action in {"moved", "split", "merged", "replaced"} and not targets:
                errors.append(f"{source}: {action} has no destination link")
            for target in targets:
                if not target.exists():
                    errors.append(f"{source}: destination does not exist: {target.relative_to(root) if target.is_relative_to(root) else target}")
            if action in {"moved", "split", "merged"} and source in tracked_now:
                errors.append(f"{source}: action is {action} but source is still tracked")
            if action in {"retained", "replaced"} and source not in tracked_now:
                errors.append(f"{source}: action is {action} but source is not tracked")
            inventory.append((source, blob, "mapped", action, dest_cell))
            continue

        matches = [(pattern, kind, reason) for pattern, kind, reason in rules if fnmatch.fnmatchcase(source, pattern)]
        if len(matches) != 1:
            errors.append(f"{source}: baseline document has {len(matches)} scope classifications and no manifest disposition")
            continue
        pattern, kind, reason = matches[0]
        inventory.append((source, blob, kind, "excluded", f"{pattern}: {reason}"))

    # Assets are inventoried independently from documents. They must either be
    # present in the current tree or have an explicit migration-manifest row.
    for source, blob in sorted(baseline_assets.items()):
        if source in rows:
            action, destination = rows.pop(source)
            inventory.append((source, blob, "asset", action, destination))
        elif source in tracked_now:
            inventory.append((source, blob, "asset", "retained", "retained at original repository path"))
        else:
            matches = [(pattern, kind, reason) for pattern, kind, reason in rules if fnmatch.fnmatchcase(source, pattern)]
            if len(matches) != 1:
                errors.append(f"{source}: baseline image/diagram asset has {len(matches)} scope classifications and no migration disposition")
                continue
            pattern, kind, reason = matches[0]
            inventory.append((source, blob, kind, "excluded", f"{pattern}: {reason}"))

    for source in rows:
        errors.append(f"{source}: manifest lists no file in baseline {args.baseline}")

    for path in sorted(current_docs - set(baseline_docs)):
        if not path.startswith("docs/knowledge/"):
            errors.append(f"{path}: new Markdown document is outside the knowledge corpus and has no baseline provenance")
            continue
        text = (root / path).read_text(encoding="utf-8")
        provenance = re.findall(
            r"Crystal Forge repository file (\S+) at commit ([0-9a-f]{7,40})", text
        )
        sources = {
            unquote(src)
            for src, rev in provenance
            if args.baseline.startswith(rev) and unquote(src) in tree
        }
        if path.endswith(("/index.md", "/log.md")):
            # Navigation files are provenance-checked by parsed local links in
            # validate.py; they do not claim to originate in a single source.
            continue
        if path.endswith("/okf-conventions.md"):
            if args.baseline not in text:
                errors.append(f"{path}: conventions note does not record the full source baseline")
            continue
        if "/migration-manifest/" in path:
            for source in manifest_rows(inventory_dir):
                if source not in baseline_docs and source not in baseline_assets:
                    errors.append(f"{path}: manifest source {source} is not in the baseline inventory")
            continue
        if not sources:
            errors.append(f"{path}: new knowledge Markdown has no repository-file provenance at baseline {args.baseline}")

    output = ["source\tbaseline_blob\tclass\taction\tdestination_or_reason"]
    output.extend("\t".join(row) for row in inventory)
    rendered = "\n".join(output) + "\n"
    if args.write:
        target = Path(args.write)
        if not target.is_absolute():
            target = root / target
        target.write_text(rendered, encoding="utf-8")
    if errors:
        print("\n".join(errors))
        print(f"FAILED: {len(errors)} inventory issue(s); {len(inventory)} baseline documents/assets classified", file=sys.stderr)
        return 1
    print(f"OK: {len(baseline_docs)} baseline documents and {len(baseline_assets)} image/diagram assets classified; {len(current_docs - set(baseline_docs))} new Markdown documents provenance-checked")
    if not args.write:
        sys.stdout.write(rendered)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, subprocess.CalledProcessError) as exc:
        print(f"inventory error: {exc}", file=sys.stderr)
        raise SystemExit(2)
