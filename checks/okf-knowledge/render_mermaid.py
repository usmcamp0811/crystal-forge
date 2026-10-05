#!/usr/bin/env python3
"""Render every Mermaid diagram in repository documents and audit conversions.

Two independent checks run on the same set of documents:

1. **Render.** Every Mermaid block in every Markdown document (and every
   standalone ``.mmd`` file) renders with the pinned Mermaid CLI. A block that
   fails to render is an error. This catches wrong syntax, not wrong meaning.
2. **Conversion ledger.** A block that carries a ``%% diagram-id: <id>`` comment
   was converted from an ASCII or Unicode diagram. The ledger in
   ``checks/okf-knowledge/diagram-audit/*.tsv`` MUST hold one row for it with a
   non-empty semantic-equivalence record. A ledger row without a matching block
   is an error. A Mermaid block without a ``diagram-id`` is original Mermaid. It
   is rendered but needs no ledger row.

The ledger records a human review. This script cannot prove that a diagram
means the same as its source. It proves that the review record exists, names a
block that exists, and states the correct Mermaid type.
"""

from __future__ import annotations

import argparse
import csv
import json
import re
import subprocess
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

FENCE = re.compile(r"^\s*(`{3,}|~{3,})\s*mermaid\s*$", re.I)
DIAGRAM_ID = re.compile(r"^\s*%%\s*diagram-id:\s*(\S+)", re.M)
SKIP_DIRS = {".git", "node_modules", "target", "result", ".direnv", "__pycache__"}
LEDGER_REQUIRED = (
    "source_path",
    "source_heading",
    "source_entities_fields",
    "source_relationships_order",
    "destination_path",
    "mermaid_diagram_id",
    "mermaid_type",
    "semantic_review",
)
# Chromium runs without its setuid sandbox in the isolated Nix builder. It also
# writes shared-memory surfaces to the build's temporary filesystem because the
# builder's /dev/shm is not a reliable writable area. The renderer reads only
# repository text, so these browser flags do not widen the input trust boundary.
PUPPETEER_CONFIG = {
    "args": [
        "--no-sandbox",
        "--disable-setuid-sandbox",
        "--disable-dev-shm-usage",
    ]
}


def skipped(path: Path) -> bool:
    return any(part in SKIP_DIRS for part in path.parts)


def blocks(path: Path):
    """Yields ``(body, first_line_number)`` for each Mermaid fence in a file."""
    lines = path.read_text(encoding="utf-8").splitlines()
    i = 0
    while i < len(lines):
        match = FENCE.match(lines[i])
        if not match:
            i += 1
            continue
        marker = match.group(1)
        start = i + 1
        i += 1
        body: list[str] = []
        closer = re.compile(rf"^\s*{re.escape(marker[0])}{{{len(marker)},}}\s*$")
        while i < len(lines) and not closer.match(lines[i]):
            body.append(lines[i])
            i += 1
        yield "\n".join(body), start
        i += 1


def diagram_type(body: str) -> str:
    """Returns the first non-comment line of a Mermaid body."""
    for line in body.splitlines():
        stripped = line.strip()
        if stripped and not stripped.startswith("%%"):
            return stripped
    return ""


def sources(root: Path) -> list[Path]:
    found = [p for p in root.rglob("*") if p.suffix.lower() in {".md", ".mmd"} and p.is_file() and not skipped(p.relative_to(root))]
    return sorted(found)


def load_ledger(root: Path) -> tuple[dict[tuple[str, str], str], list[str]]:
    expected: dict[tuple[str, str], str] = {}
    errors: list[str] = []
    ledger_dir = root / "checks/okf-knowledge/diagram-audit"
    for ledger in sorted(ledger_dir.glob("*.tsv")):
        if ledger.name == "exceptions.tsv":
            continue
        with ledger.open(encoding="utf-8", newline="") as handle:
            reader = csv.DictReader(handle, delimiter="\t")
            missing = [c for c in LEDGER_REQUIRED if not reader.fieldnames or c not in reader.fieldnames]
            if missing:
                errors.append(f"{ledger.name}: missing ledger columns {missing}")
                continue
            for row in reader:
                ident = row["mermaid_diagram_id"].strip()
                where = f"{ledger.name}: {ident or '(no id)'}"
                for field in LEDGER_REQUIRED:
                    if not row[field].strip():
                        errors.append(f"{where} has an empty {field}")
                key = (row["destination_path"].strip(), ident)
                # Two source documents can hold the same original diagram, so
                # several rows may name one destination block. They MUST agree
                # on the Mermaid type.
                if key in expected and expected[key] != row["mermaid_type"].strip():
                    errors.append(f"{where} disagrees on the Mermaid type for {key[0]}")
                expected[key] = row["mermaid_type"].strip()
    return expected, errors


def observed_blocks(root: Path) -> tuple[dict[tuple[str, str], str], list[str]]:
    observed: dict[tuple[str, str], str] = {}
    errors: list[str] = []
    for path in sources(root):
        if path.suffix.lower() != ".md":
            continue
        relative = path.relative_to(root).as_posix()
        for body, line in blocks(path):
            found = DIAGRAM_ID.search(body)
            if not found:
                continue
            key = (relative, found.group(1))
            if key in observed:
                errors.append(f"{relative}:{line}: duplicate diagram-id {found.group(1)}")
            observed[key] = diagram_type(body[found.end():])
    return observed, errors


def check_ledger(root: Path) -> list[str]:
    expected, errors = load_ledger(root)
    observed, more = observed_blocks(root)
    errors.extend(more)
    for key, kind in sorted(expected.items()):
        if key not in observed:
            errors.append(f"ledger row has no Mermaid block: {key[0]}#{key[1]}")
            continue
        directive = observed[key]
        first = directive.split(maxsplit=1)[0] if directive else ""
        wanted = kind.split(maxsplit=1)[0]
        if first != wanted:
            errors.append(f"{key[0]}#{key[1]}: Mermaid type {first!r}, ledger says {kind!r}")
    for key in sorted(observed.keys() - expected.keys()):
        errors.append(f"Mermaid block has a diagram-id but no ledger row: {key[0]}#{key[1]}")
    return errors


def render_one(args: tuple[Path, str, int, Path, str, Path]) -> str | None:
    source, body, index, out, renderer, config = args
    with tempfile.NamedTemporaryFile("w", suffix=".mmd", encoding="utf-8", dir=out, delete=False) as tmp:
        tmp.write(body + "\n")
        input_path = Path(tmp.name)
    svg = input_path.with_suffix(".svg")
    try:
        for attempt in range(3):
            result = subprocess.run(
                [renderer, "-p", str(config), "-i", str(input_path), "-o", str(svg)],
                capture_output=True,
                text=True,
            )
            if result.returncode == 0:
                break
            transient = "svg element not in render tree" in (result.stderr + result.stdout)
            if not transient or attempt == 2:
                break
            # Puppeteer can report this transient layout race under a loaded
            # virtualized builder. Retry only that renderer error; syntax and
            # parser failures remain immediate failures.
            time.sleep(0.1 * (attempt + 1))
    finally:
        input_path.unlink(missing_ok=True)
    svg.unlink(missing_ok=True)
    if result.returncode:
        detail = (result.stderr or result.stdout).strip().splitlines()
        useful = "\n".join(detail[-12:]) if detail else "renderer failed"
        return f"{source} block {index + 1}: {useful}"
    return None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", default=".")
    parser.add_argument("--renderer", default="mmdc")
    parser.add_argument("--out", required=True)
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("--ledger-only", action="store_true", help="skip rendering")
    args = parser.parse_args()
    root, out = Path(args.repo_root).resolve(), Path(args.out)
    out.mkdir(parents=True, exist_ok=True)

    errors = check_ledger(root)
    total = 0
    if not args.ledger_only:
        config = out / "puppeteer-config.json"
        config.write_text(json.dumps(PUPPETEER_CONFIG), encoding="utf-8")
        work = []
        for source in sources(root):
            if source.suffix.lower() == ".mmd":
                items = [(source.read_text(encoding="utf-8"), 1)]
            else:
                items = list(blocks(source))
            rel = source.relative_to(root).as_posix()
            for index, (body, _line) in enumerate(items):
                work.append((rel, body, index, out, args.renderer, config))
        total = len(work)
        with ThreadPoolExecutor(max_workers=max(1, args.jobs)) as pool:
            for result in pool.map(render_one, work):
                if result:
                    errors.append(result)

    report = out / "mermaid-report.json"
    report.write_text(json.dumps({"blocks": total, "errors": errors}, indent=2) + "\n", encoding="utf-8")
    print(f"Mermaid: {total} block(s) rendered, {len(errors)} error(s); report {report}")
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
