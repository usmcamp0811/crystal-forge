#!/usr/bin/env python3
"""Validate immutable-baseline source blocks against migration mappings.

The baseline is read from Git objects, never from the working tree. Every
baseline Markdown document must have one manifest row or one narrow scope
classification. Every H2 source block must then have an explicit destination
entry in that row's detailed ``Source inventory`` section. Prose matching is
ordered and multiplicity-aware; fenced blocks and Markdown tables retain their
punctuation and content.
"""

from __future__ import annotations

import argparse
import csv
import fnmatch
import hashlib
import json
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path, PurePosixPath, PureWindowsPath
from typing import NamedTuple

import diagram_scan
from validate import slugify

DOC_SUFFIXES = {".md", ".mdx", ".rst", ".adoc", ".asciidoc"}
HEADING = re.compile(r"^##\s+(.+?)\s*#*\s*$")
LINK = re.compile(r"\]\(([^)\s]+)\)")
FENCE = re.compile(r"^\s*(`{3,}|~{3,})(.*)$")
H1 = re.compile(r"^#\s+\S")
RULE = re.compile(r"^\s{0,3}([-*_])(?:\s*\1){2,}\s*$")

# Name of the synthetic block that holds the content before the first H2.
# INVARIANT: no real H2 may use this name; ``split_document`` rejects it.
PREAMBLE_HEADING = "__preamble__"
CLEANUP_RECORD = "docs/knowledge/meta/cleanup-record.md"


def git(*args: str, binary: bool = False):
    return subprocess.run(["git", *args], check=True, capture_output=True,
                          text=not binary).stdout


def require_revision(revision: str) -> None:
    git("cat-file", "-e", f"{revision}^{{commit}}")


def baseline_paths(revision: str) -> list[str]:
    raw = git("ls-tree", "-r", "--full-tree", "--name-only", "-z", revision, binary=True)
    return [p.decode("utf-8", "surrogateescape") for p in raw.split(b"\0") if p
            and Path(p.decode("utf-8", "surrogateescape")).suffix.lower() in DOC_SUFFIXES]


def baseline_text(revision: str, path: str) -> str:
    """Returns the immutable blob text without newline translation.

    Block hashes in the ledgers are computed over these exact characters, so
    the blob is decoded from bytes instead of using text-mode output.
    """
    return git("show", f"{revision}:{path}", binary=True).decode("utf-8")


def split_blocks(text: str) -> list[tuple[str, str]]:
    """Return H2 blocks including their heading, preserving source order."""
    lines = text.splitlines(keepends=True)
    starts = [i for i, line in enumerate(lines) if HEADING.match(line.rstrip("\r\n"))]
    result = []
    for n, start in enumerate(starts):
        heading = HEADING.match(lines[start].rstrip("\r\n")).group(1)
        end = starts[n + 1] if n + 1 < len(starts) else len(lines)
        result.append((heading, "".join(lines[start:end])))
    return result


def preamble_content(text: str) -> str:
    """Returns the preservation-relevant text before the first H2, or ``""``.

    Front matter, H1 title lines, thematic breaks, and blank lines are not
    content: a destination concept has its own title and metadata. Everything
    else is content, including H3 and deeper headings, fenced code (kept
    byte-exact, including indentation), tables, lists, and prose. A document
    without an H2 is entirely preamble.
    """
    lines = text.splitlines(keepends=True)
    first_h2 = next((i for i, l in enumerate(lines) if HEADING.match(l.rstrip("\r\n"))), len(lines))
    head = lines[:first_h2]
    if head and head[0].strip() == "---":
        closing = next((i for i in range(1, len(head)) if head[i].strip() == "---"), None)
        if closing is not None:
            head = head[closing + 1:]
    kept: list[str] = []
    fence: str | None = None
    for line in head:
        opened = FENCE.match(line.rstrip("\r\n"))
        if fence is None and opened:
            fence = opened.group(1)[0]
        elif fence is not None and opened and opened.group(1)[0] == fence and not opened.group(2).strip():
            fence = None
        elif fence is None and (not line.strip() or H1.match(line) or RULE.match(line.rstrip("\r\n"))):
            continue
        kept.append(line)
    return "".join(kept)


def split_document(text: str) -> list[tuple[str, str]]:
    """Returns the preamble block, when it has content, then every H2 block.

    The preamble is the synthetic block ``PREAMBLE_HEADING``. A document with
    no H2 yields one preamble block, so its whole body is preservation-checked.
    The H2 blocks equal ``split_blocks`` output, so their hashes are unchanged.
    """
    blocks = split_blocks(text)
    if any(heading == PREAMBLE_HEADING for heading, _ in blocks):
        raise ValueError(f"an H2 heading must not be named {PREAMBLE_HEADING!r}")
    preamble = preamble_content(text)
    return ([(PREAMBLE_HEADING, preamble)] if preamble else []) + blocks


def tokenize_prose(text: str) -> list[str]:
    """Preserve punctuation and token order while permitting whitespace reflow."""
    return re.findall(r"[^\W_]+(?:['’][^\W_]+)*|[^\s\w]", text, flags=re.UNICODE)


def protected_blocks(text: str) -> list[str]:
    """Return code fences and table rows as exact, ordered protected content."""
    lines = text.splitlines()
    found: list[str] = []
    in_fence = False
    marker = ""
    table: list[str] = []
    for line in lines:
        fm = FENCE.match(line)
        if fm:
            if not in_fence:
                in_fence, marker = True, fm.group(1)[0]
                found.append("FENCE\n")
            else:
                in_fence = False
                found.append("FENCE_END\n")
            continue
        if in_fence:
            found.append(line + "\n")
            continue
        if line.lstrip().startswith("|"):
            found.append("TABLE:" + line + "\n")
    return found


def preserved(source: str, destinations: list[str]) -> bool:
    joined = "\n".join(destinations)
    src_tokens = tokenize_prose(source)
    dest_tokens = tokenize_prose(joined)
    cursor = 0
    for token in src_tokens:
        try:
            cursor = dest_tokens.index(token, cursor) + 1
        except ValueError:
            return False
    # Protected syntax is compared byte-for-byte, in order and with repeats.
    protected = protected_blocks(source)
    available = protected_blocks(joined)
    cursor = 0
    for block in protected:
        try:
            cursor = available.index(block, cursor) + 1
        except ValueError:
            return False
    return True


class Adjustment(NamedTuple):
    """One hash-bound transformation of one baseline block.

    ``old_text`` and ``new_text`` are used only by ``replace-exact-text``.
    """

    digest: str
    adjustment_id: str
    transformation: str
    old_text: str = ""
    new_text: str = ""


def load_source_adjustments(path: Path) -> dict[tuple[str, str], Adjustment]:
    """Loads exact, hash-bound transformations authorized by cleanup records.

    The optional columns ``old_text`` and ``new_text`` hold JSON strings. They
    are required for ``replace-exact-text`` and forbidden for every other
    transformation.
    """
    adjustments: dict[tuple[str, str], Adjustment] = {}
    with path.open(encoding="utf-8", newline="") as handle:
        # QUOTE_NONE: the JSON text columns contain double quotes that must
        # reach ``json.loads`` unchanged.
        reader = csv.DictReader(handle, delimiter="\t", quoting=csv.QUOTE_NONE)
        required = {
            "source_path", "source_heading", "baseline_block_sha256",
            "adjustment_id", "transformation", "reason",
        }
        if not reader.fieldnames or not required.issubset(reader.fieldnames):
            raise ValueError(f"{path}: missing source-adjustment columns")
        for row in reader:
            key = (row["source_path"], row["source_heading"])
            if key in adjustments:
                raise ValueError(f"{path}: duplicate source adjustment {key}")
            if not all((row[name] or "").strip() for name in required):
                raise ValueError(f"{path}: incomplete source adjustment {key}")
            texts = []
            for column in ("old_text", "new_text"):
                raw = row.get(column) or ""
                try:
                    texts.append(json.loads(raw) if raw else "")
                except json.JSONDecodeError as exc:
                    raise ValueError(f"{path}: {key}: {column} is not a JSON string: {exc}") from exc
                if raw and not isinstance(texts[-1], str):
                    raise ValueError(f"{path}: {key}: {column} must be a JSON string")
            exact = row["transformation"] == "replace-exact-text"
            if exact and not texts[0]:
                raise ValueError(f"{path}: {key}: replace-exact-text needs a non-empty old_text")
            if not exact and any(texts):
                raise ValueError(f"{path}: {key}: old_text and new_text apply only to replace-exact-text")
            adjustments[key] = Adjustment(
                row["baseline_block_sha256"], row["adjustment_id"], row["transformation"], *texts
            )
    return adjustments


def apply_source_adjustment(source: str, adjustment: tuple, block: str) -> str:
    """Applies one named transformation to its exact baseline block only.

    ``replace-exact-text`` replaces one stale claim by its corrected text. The
    claim MUST occur exactly once in the block. Every other part of the block
    stays subject to the normal preservation comparison.
    """
    digest, _adjustment_id, transformation, old_text, new_text = Adjustment(*adjustment)
    if hashlib.sha256(block.encode("utf-8")).hexdigest() != digest:
        raise ValueError("baseline block hash does not match the authorized adjustment")
    if transformation == "replace-exact-text":
        if block.count(old_text) != 1:
            raise ValueError(f"expected exactly one occurrence of the corrected claim, found {block.count(old_text)}")
        return block.replace(old_text, new_text)
    if transformation != "normalize-absolute-web-ui-path" or source != "docs/design/FIGMA_CLAUDE_WORKFLOW.md":
        if transformation == "normalize-contributing-test-routes" and source == "CONTRIBUTING.md":
            replacements = (
                ("packages/cf-test-modules", "packages/cf-test-suite"),
                ("nix build .#checks.x86_64-linux.database",
                 "nix run .#cf-test-suite.runTests -- -vvv -m database"),
                ("- Server tests: `nix build .#checks.x86_64-linux.server`",
                 "- Server regression tests: `nix build .#checks.x86_64-linux.server-regressions`"),
                ("- Builder tests: `nix build .#checks.x86_64-linux.builder`",
                 "- Integration VM: `nix build .#checks.x86_64-linux.integration`"),
                ("- Cache tests: `nix build .#checks.x86_64-linux.s3-cache` or `.#checks.x86_64-linux.attic-cache`",
                 "- Python server, builder, cache, and database tests: `nix run .#cf-test-suite.runTests -- -vvv`"),
                ("- Full test suite: `nix flake check`",
                 "- Full flake checks: `nix flake check`"),
                ("See the [Test Plan](docs/test_plan.md) for detailed testing guidance.",
                 "See the [testing guide](docs/knowledge/testing/test-plan.md) and\n"
                 "[flake-check catalog](docs/knowledge/testing/flake-checks.md) for current\n"
                 "testing guidance."),
            )
            output = block
            for old, new in replacements:
                if old not in output:
                    raise ValueError(f"expected contribution guidance text was not found: {old[:60]!r}")
                output = output.replace(old, new)
            return output
        if transformation == "normalize-contributing-doc-path" and source == "CONTRIBUTING.md":
            old = "docs/frontend-component-standards.md"
            if block.count(old) != 1:
                raise ValueError("expected one obsolete frontend standards path")
            return block.replace(old, "docs/knowledge/ui/component-isolation-standards.md")
        raise ValueError(f"unsupported source adjustment {transformation!r} for {source}")
    normalized, count = re.subn(
        r"(?m)^(- Code: )`/home/[^`\n]+/packages/web-ui/`$",
        r"\1`packages/web-ui/`",
        block,
    )
    if count != 1:
        raise ValueError("expected exactly one absolute web-UI source path")
    return normalized


def remove_converted_diagrams(
    source: str,
    heading: str,
    block: str,
    destinations: list[tuple[Path, str]],
    rows: list[dict[str, str]],
) -> tuple[str, list[str]]:
    """Removes only source figures with a matching semantic Mermaid record.

    The diagram ledger MUST account for every structural diagram removed from
    preservation comparison. Remaining prose, code, tables, and ordering stay
    subject to the normal source-block comparison.
    """
    matching = [
        row for row in rows
        if row["source_path"] == source
    ]
    if not matching:
        return block, [text for _path, text in destinations]
    source_sections = {heading}
    source_sections.update(
        match.group(1).strip()
        for line in block.splitlines()
        if (match := re.match(r"^\s*#{1,6}\s+(.+?)\s*#*\s*$", line))
    )
    matching = [row for row in matching if row["source_heading"] in source_sections]
    if not matching:
        return block, [text for _path, text in destinations]
    candidates = [
        item for item in diagram_scan.scan_text(source, block)
        if item.heading in source_sections and item.context in {"fenced code block", "unfenced figure"}
    ]
    if len(candidates) < len(matching):
        # Some reviewed source diagrams are text-only priority or hierarchy
        # blocks. The explicit ledger row classifies a text fence as a diagram
        # when scanner shape heuristics alone cannot do so.
        lines = block.splitlines()
        fenced_text: list[diagram_scan.Candidate] = []
        i = 0
        active_heading = heading
        while i < len(lines):
            heading_match = re.match(r"^\s*#{1,6}\s+(.+?)\s*#*\s*$", lines[i])
            if heading_match:
                active_heading = heading_match.group(1).strip()
            opened = FENCE.match(lines[i])
            if not opened:
                i += 1
                continue
            marker = opened.group(1)
            language = opened.group(2).strip().split(maxsplit=1)[0].lower() if opened.group(2).strip() else ""
            j = i + 1
            while j < len(lines) and not re.match(rf"^\s*{re.escape(marker[0])}{{{len(marker)},}}\s*$", lines[j]):
                j += 1
            if language in {"", "text", "plaintext", "ascii"} and active_heading in source_sections:
                fenced_text.append(diagram_scan.Candidate(
                    source, active_heading, i + 1, min(j + 1, len(lines)),
                    "\n".join(lines[i + 1:j]), "fenced source block", True, language
                ))
            i = min(j + 1, len(lines))
        missing_count = len(matching) - len(candidates)
        if len(fenced_text) == missing_count:
            candidates.extend(fenced_text)
    if len(candidates) != len(matching):
        raise ValueError(
            f"diagram ledger has {len(matching)} rows for {len(candidates)} structural source figure(s)"
        )

    lines = block.splitlines(keepends=True)
    remove: set[int] = set()
    for item in candidates:
        remove.update(range(item.start - 1, min(item.end, len(lines))))
    source_without_figures = "".join(line for index, line in enumerate(lines) if index not in remove)

    remaining_destinations = [text for _path, text in destinations]
    used_destination_paths = {path for path, _text in destinations}
    errors: list[str] = []
    for row in matching:
        target = Path(row["destination_path"])
        if target not in used_destination_paths:
            errors.append(f"diagram ledger destination {target} is not linked by the source-block map")
            continue
        if not row["source_entities_fields"].strip() or not row["source_relationships_order"].strip() or not row["semantic_review"].strip():
            errors.append(f"diagram ledger row {row['mermaid_diagram_id']} lacks semantic review data")
            continue
        idx = next(i for i, (path, _text) in enumerate(destinations) if path == target)
        remaining_destinations[idx] = remove_mermaid_block(
            remaining_destinations[idx], row["mermaid_diagram_id"]
        )
    return source_without_figures, remaining_destinations


def remove_mermaid_block(text: str, diagram_id: str) -> str:
    """Removes one Mermaid fence only when its exact diagram id matches."""
    lines = text.splitlines(keepends=True)
    output: list[str] = []
    i = 0
    removed = False
    while i < len(lines):
        opened = FENCE.match(lines[i])
        if not opened or opened.group(2).strip().split(maxsplit=1)[0:1] != ["mermaid"]:
            output.append(lines[i])
            i += 1
            continue
        marker = opened.group(1)
        j = i + 1
        while j < len(lines) and not re.match(rf"^\s*{re.escape(marker[0])}{{{len(marker)},}}\s*$", lines[j]):
            j += 1
        body = "".join(lines[i + 1:j])
        if re.search(rf"^\s*%%\s*diagram-id:\s*{re.escape(diagram_id)}\s*$", body, re.M):
            if removed:
                raise ValueError(f"duplicate Mermaid diagram id {diagram_id}")
            removed = True
        else:
            output.extend(lines[i:min(j + 1, len(lines))])
        i = min(j + 1, len(lines))
    if not removed:
        raise ValueError(f"Mermaid diagram id {diagram_id} was not found in destination")
    return "".join(output)


SHA256_HEX = re.compile(r"^[0-9a-f]{64}$")
CLEANUP_ID = re.compile(r"^C-\d{3}$")
SEMANTIC_COLUMNS = (
    "source_path", "source_heading", "baseline_block_sha256",
    "adjustment_id", "destination_path", "destination_heading",
    "destination_block_sha256", "transformation", "reason",
)


def validate_destination_path(root: Path, value: str) -> str | None:
    """Returns a diagnostic when ``value`` is not a normalized in-repo path.

    A valid destination is a repository-relative POSIX path with no ``.`` or
    ``..`` segments, no backslashes, and a final resolved location inside
    ``root``. Symbolic links that leave ``root`` are rejected.
    """
    if value.startswith("/") or PureWindowsPath(value).is_absolute() or "\\" in value:
        return f"destination_path must be repository-relative and use '/': {value!r}"
    parts = PurePosixPath(value)
    if ".." in parts.parts:
        return f"destination_path must not contain '..': {value!r}"
    if parts.as_posix() != value:
        return f"destination_path is not normalized: {value!r}"
    try:
        (root / value).resolve().relative_to(root.resolve())
    except ValueError:
        return f"destination_path resolves outside the repository: {value!r}"
    return None


def load_semantic_replacements(root: Path) -> tuple[list[dict[str, str]], list[str]]:
    """Loads owner-authorized semantic replacement rows.

    Returns the schema-valid rows and one diagnostic per rejected row. This
    function never raises for malformed ledger content, so the caller can fail
    the audit with deterministic diagnostics instead of a traceback. A rejected
    row is not returned and therefore cannot authorize any replacement.
    """
    path = root / "checks/okf-knowledge/semantic-replacements.tsv"
    if not path.is_file():
        return [], []
    name = "semantic-replacements.tsv"
    rows: list[dict[str, str]] = []
    errors: list[str] = []
    with path.open(encoding="utf-8", newline="") as handle:
        reader = csv.DictReader(handle, delimiter="\t")
        if reader.fieldnames is None:
            return [], [f"{name}: empty file; expected a header row"]
        missing = [c for c in SEMANTIC_COLUMNS if c not in reader.fieldnames]
        if missing:
            return [], [f"{name}: header is missing required columns {missing}"]
        seen: set[tuple[str, str]] = set()
        for number, row in enumerate(reader, start=2):
            where = f"{name}:{number}"
            if None in row:
                errors.append(f"{where}: row has more cells than the header")
                continue
            absent = [c for c in SEMANTIC_COLUMNS if row.get(c) is None]
            if absent:
                errors.append(f"{where}: row is missing required fields {absent}")
                continue
            empty = [c for c in SEMANTIC_COLUMNS if not row[c].strip()]
            if empty:
                errors.append(f"{where}: empty required fields {empty}")
                continue
            padded = [c for c in SEMANTIC_COLUMNS if row[c] != row[c].strip()]
            if padded:
                errors.append(f"{where}: fields have leading or trailing whitespace {padded}")
                continue
            problems = [
                f"{field} must be 64 lowercase hexadecimal characters"
                for field in ("baseline_block_sha256", "destination_block_sha256")
                if not SHA256_HEX.match(row[field])
            ]
            if row["transformation"] != "semantic-replacement":
                problems.append(
                    f"unsupported transformation {row['transformation']!r}; "
                    "expected 'semantic-replacement'"
                )
            if not CLEANUP_ID.match(row["adjustment_id"]):
                problems.append(f"adjustment_id must match C-NNN, got {row['adjustment_id']!r}")
            path_problem = validate_destination_path(root, row["destination_path"])
            if path_problem:
                problems.append(path_problem)
            if problems:
                errors.append(f"{where}: " + "; ".join(problems))
                continue
            key = (row["source_path"], row["source_heading"])
            if key in seen:
                errors.append(f"{where}: duplicate semantic replacement key {key}")
                continue
            seen.add(key)
            rows.append(row)
    return rows, errors


def verify_semantic_replacement(
    row: dict[str, str],
    raw_block: str,
    allowed: set[tuple[str, str]],
    root: Path,
    cleanup_text: str,
    adjusted: bool,
) -> str | None:
    """Returns a diagnostic when ``row`` does not authorize ``raw_block``.

    The caller MUST pass ``allowed`` after resolving the migration manifest
    mapping for the exact source H2. ``allowed`` holds the repository-relative
    destination path and GitLab anchor of each destination that the manifest
    maps for that H2. The row can authorize only one of those destinations.
    ``raw_block`` is the immutable baseline block before any source adjustment.
    """
    if adjusted:
        return "semantic replacement overlaps a source adjustment; one block cannot use both"
    if f"| {row['adjustment_id']} |" not in cleanup_text:
        return f"cleanup-record.md has no entry {row['adjustment_id']}"
    actual = hashlib.sha256(raw_block.encode("utf-8")).hexdigest()
    if actual != row["baseline_block_sha256"]:
        return (f"baseline_block_sha256 mismatch: ledger {row['baseline_block_sha256']}, "
                f"baseline block {actual}")
    destination = row["destination_path"]
    anchor = slugify(row["destination_heading"])
    if (destination, anchor) not in allowed:
        return (f"destination {destination}#{anchor} is not a manifest-mapped destination "
                f"for this source H2")
    target = root / destination
    if not target.is_file():
        return f"destination {destination} does not exist"
    matches = [b for h, b in split_blocks(target.read_text(encoding="utf-8"))
               if h == row["destination_heading"]]
    if len(matches) != 1:
        return (f"destination {destination} must have exactly one H2 "
                f"{row['destination_heading']!r}, found {len(matches)}")
    actual = hashlib.sha256(matches[0].encode("utf-8")).hexdigest()
    if actual != row["destination_block_sha256"]:
        return (f"destination_block_sha256 mismatch: ledger {row['destination_block_sha256']}, "
                f"destination block {actual}")
    return None


def load_diagram_rows(root: Path) -> list[dict[str, str]]:
    """Loads semantic conversion records for baseline source diagrams."""
    rows = []
    for path in sorted((root / "checks/okf-knowledge/diagram-audit").glob("*.tsv")):
        if path.name == "exceptions.tsv":
            continue
        with path.open(encoding="utf-8", newline="") as handle:
            rows.extend(csv.DictReader(handle, delimiter="\t"))
    return rows


def parse_manifest(bundle: Path) -> dict[str, tuple[Path, str, str, list[tuple[str, str]]]]:
    rows = {}
    manifest_dir = bundle / "meta" / "migration-manifest"
    for manifest in sorted(manifest_dir.glob("*.md")):
        if manifest.name in {"index.md", "log.md"}:
            continue
        lines = manifest.read_text(encoding="utf-8").splitlines()
        source = None
        for i, line in enumerate(lines):
            if not re.match(r"^\|\s*Original\s*\|", line):
                continue
            for row in lines[i + 2:]:
                if not row.startswith("|"):
                    break
                cells = [c.strip() for c in re.split(r"(?<!\\)\|", row.strip().strip("|"))]
                if len(cells) != 4:
                    raise ValueError(f"{manifest}: malformed source row")
                path, dest, action, coverage = cells
                path = path.strip("`")
                if path in rows:
                    raise ValueError(f"duplicate manifest source {path}")
                if coverage != "complete":
                    raise ValueError(f"{manifest}: {path} coverage is not complete")
                rows[path] = (manifest, dest, action, [])
            break

        current_source = None
        in_detail = False
        for line in lines:
            source_heading = re.match(r"^###\s+`([^`]+)`", line)
            if source_heading:
                current_source = source_heading.group(1)
                continue
            if line.startswith("## Source inventory"):
                in_detail = True
                continue
            if in_detail and current_source in rows:
                match = re.match(
                    r"\s*\|\s*`##\s+(.+?)`(?:\s*\([^|]*\))?\s*\|\s*(.+?)\s*\|",
                    line,
                )
                if match:
                    rows[current_source][3].append(match.groups())
    return rows


def mapped_destinations_for(
    source: str,
    heading: str,
    manifest: Path,
    map_cell: str,
    root: Path,
    errors: list[str],
) -> tuple[list[tuple[Path, str]], set[tuple[str, str]]]:
    """Resolves the manifest destinations of one source H2.

    Returns the destination texts used for preservation comparison and the
    ``(repository-relative path, anchor)`` pairs that the manifest maps for the
    H2. Missing anchors and missing files are appended to ``errors``.
    """
    destinations: list[tuple[Path, str]] = []
    allowed: set[tuple[str, str]] = set()
    for link in LINK.findall(map_cell):
        target, _, anchor = link.partition("#")
        if re.match(r"^[A-Za-z][A-Za-z0-9+.-]*:", target):
            continue
        resolved = (manifest.parent / target).resolve()
        if resolved == (root / CLEANUP_RECORD).resolve():
            # SECURITY: The cleanup record documents corrections. It is
            # evidence for an annotation, never a destination that can
            # satisfy preservation of a source block.
            continue
        if not anchor:
            errors.append(f"{source}: H2 {heading!r} mapping lacks an exact destination anchor")
        if not resolved.is_file():
            errors.append(f"{source}: H2 {heading!r} destination is missing: {target}")
            continue
        try:
            relative = resolved.relative_to(root)
        except ValueError:
            errors.append(f"{source}: H2 {heading!r} destination is outside the repository: {target}")
            continue
        destinations.append((relative, resolved.read_text(encoding="utf-8")))
        allowed.add((relative.as_posix(), anchor))
    return destinations, allowed


def audit(base: str, bundle: Path, scope_file: Path, root: Path, only: tuple[str, ...] = ()) -> int:
    """Checks every baseline document against the migration manifests.

    Returns 0 when preservation is proven, 1 when any block is lost or any
    ledger is invalid, 5 when ``only`` limited the run and no error was found,
    and raises for unreadable inputs (see ``main``).

    ``only`` is a development aid. It limits checking to baseline paths that
    contain one of the given substrings. A limited run is never a preservation
    proof, so it never returns 0. CI MUST NOT pass ``only``.
    """
    require_revision(base)
    errors: list[str] = []
    rows = parse_manifest(bundle)
    diagram_rows = load_diagram_rows(root)
    adjustment_file = root / "checks/okf-knowledge/source-adjustments.tsv"
    adjustments = load_source_adjustments(adjustment_file) if adjustment_file.is_file() else {}
    semantic_by_key: dict[tuple[str, str], dict[str, str]] = {}
    semantic_rows, semantic_errors = load_semantic_replacements(root)
    errors.extend(semantic_errors)
    for row in semantic_rows:
        semantic_by_key[(row["source_path"], row["source_heading"])] = row
    cleanup_record = bundle / "meta/cleanup-record.md"
    cleanup_text = cleanup_record.read_text(encoding="utf-8") if cleanup_record.is_file() else ""
    used_adjustments: set[tuple[str, str]] = set()
    used_semantic: set[tuple[str, str]] = set()
    baseline = baseline_paths(base)
    if only:
        baseline = [p for p in baseline if any(token in p for token in only)]
    scoped_patterns = []
    for line_no, line in enumerate(scope_file.read_text(encoding="utf-8").splitlines(), 1):
        if not line or line.startswith("#"):
            continue
        cells = line.split("\t")
        if len(cells) != 3 or not all(cells):
            errors.append(f"{scope_file}:{line_no}: expected path pattern, classification, reason")
            continue
        scoped_patterns.append(cells[0])
    for source in baseline:
        if source not in rows:
            matches = [pattern for pattern in scoped_patterns if fnmatch.fnmatchcase(source, pattern)]
            if len(matches) != 1:
                errors.append(f"{source}: baseline Markdown document has no migration row or exactly one narrow scope classification")
            continue
        manifest, dest_cell, action, detail = rows[source]
        baseline_bytes = git("show", f"{base}:{source}", binary=True)
        current_path = root / source
        identical_retained = action == "retained" and current_path.is_file() and current_path.read_bytes() == baseline_bytes
        if action == "retained" and not current_path.is_file():
            errors.append(f"{source}: retained source is missing from the current tree")
            continue
        if identical_retained:
            # Retained documents have not changed, so the immutable Git blob is
            # itself the complete preservation proof. Requiring an H2 map here
            # would make unchanged root entrypoints and check READMEs fail.
            continue
        if action == "excluded":
            matches = [pattern for pattern in scoped_patterns if fnmatch.fnmatchcase(source, pattern)]
            if len(matches) != 1:
                errors.append(f"{source}: excluded disposition is not supported by exactly one scope rule")
            continue
        document_destinations: list[tuple[Path, str]] = []
        for link in LINK.findall(dest_cell):
            target = link.partition("#")[0]
            if re.match(r"^[A-Za-z][A-Za-z0-9+.-]*:", target):
                continue
            resolved = (manifest.parent / target).resolve()
            if not resolved.is_file():
                errors.append(f"{source}: missing destination {target}")
                continue
            document_destinations.append((resolved.relative_to(root), resolved.read_text(encoding="utf-8")))

        blocks = split_document(baseline_text(base, source))
        h2_headings = [heading for heading, _block in blocks if heading != PREAMBLE_HEADING]
        mapped = [heading for heading, _dest in detail]
        retained_blocks: dict[str, str] = {}
        retained_text = ""
        if action == "retained":
            retained_text = current_path.read_text(encoding="utf-8")
            current_blocks = split_blocks(retained_text)
            retained_blocks = {heading: text for heading, text in current_blocks}
            if len(retained_blocks) != len(current_blocks):
                errors.append(f"{source}: retained source has duplicate H2 headings; cannot map blocks by heading")
        if action != "retained" and Counter(mapped) != Counter(h2_headings):
            errors.append(f"{source}: detailed source map does not map each H2 block exactly once")
        for heading, block in blocks:
            if heading == PREAMBLE_HEADING:
                # The preamble has no per-block manifest row. It is compared
                # with every destination of the document row, or with the
                # current file for a retained document.
                targets = [(Path(source), retained_text)] if action == "retained" else document_destinations
                if not targets:
                    errors.append(f"{source}: preamble has no resolvable destination")
                    continue
                try:
                    preamble, remaining = remove_converted_diagrams(
                        source, heading, block, targets, diagram_rows
                    )
                except ValueError as exc:
                    errors.append(f"{source}: preamble: {exc}")
                    continue
                if not preserved(preamble, remaining):
                    errors.append(f"{source}: content/order/punctuation lost in the preamble before the first H2")
                continue
            # Resolve the manifest destinations of this exact H2 before any
            # semantic-replacement or content comparison.
            if action == "retained" and heading in retained_blocks:
                mapped_destinations = [(Path(source), retained_blocks[heading])]
                allowed = {(source, slugify(heading))}
            elif mapped.count(heading) != 1:
                errors.append(f"{source}: unmapped H2 source block {heading!r}")
                continue
            else:
                map_cell = next(cell for name, cell in detail if name == heading)
                mapped_destinations, allowed = mapped_destinations_for(
                    source, heading, manifest, map_cell, root, errors
                )
            key = (source, heading)
            raw_block = block
            if key in adjustments:
                adjustment_id = adjustments[key].adjustment_id
                if f"| {adjustment_id} |" not in cleanup_text:
                    errors.append(f"{source}: adjustment {adjustment_id} is not recorded in cleanup-record.md")
                    continue
                try:
                    block = apply_source_adjustment(source, adjustments[key], raw_block)
                except ValueError as exc:
                    errors.append(f"{source}: {heading!r}: {exc}")
                    continue
                used_adjustments.add(key)

            replacement = semantic_by_key.get(key)
            if replacement is not None:
                problem = verify_semantic_replacement(
                    replacement, raw_block, allowed, root, cleanup_text, key in adjustments
                )
                if problem:
                    errors.append(f"{source}: {heading!r}: semantic replacement {replacement['adjustment_id']}: {problem}")
                else:
                    used_semantic.add(key)
                continue
            if not mapped_destinations:
                errors.append(f"{source}: H2 {heading!r} has no resolvable destination")
                continue
            try:
                block, destinations_without_diagrams = remove_converted_diagrams(
                    source, heading, block, mapped_destinations, diagram_rows
                )
            except ValueError as exc:
                errors.append(f"{source}: H2 {heading!r}: {exc}")
                continue
            if not preserved(block, destinations_without_diagrams):
                errors.append(f"{source}: content/order/punctuation lost in H2 {heading!r}")

    in_scope = lambda path: not only or any(token in path for token in only)  # noqa: E731
    for key in sorted(k for k in adjustments.keys() - used_adjustments if in_scope(k[0])):
        errors.append(f"{key[0]}: stale source adjustment for H2 {key[1]!r}")
    for key in sorted(k for k in semantic_by_key.keys() - used_semantic if in_scope(k[0])):
        row = semantic_by_key[key]
        # A row whose verification failed already has a specific diagnostic.
        if not any(f"semantic replacement {row['adjustment_id']}:" in e for e in errors):
            errors.append(
                f"{key[0]}: stale semantic replacement {row['adjustment_id']} for H2 {key[1]!r}: "
                "no baseline block uses it"
            )

    if errors:
        print("\n".join(errors))
        print(f"FAILED: {len(errors)} source-block preservation error(s)", file=sys.stderr)
        return 1
    if only:
        print(f"PARTIAL (--only): {len(baseline)} document(s) had no error; this is NOT a preservation proof")
        return 5
    print(f"OK: {len(baseline)} baseline Markdown documents checked against source-block maps")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", required=True)
    parser.add_argument("--bundle", default="docs/knowledge")
    parser.add_argument("--scope", default="checks/okf-knowledge/source-scope.tsv")
    parser.add_argument("--repo-root", default=".")
    parser.add_argument("--only", action="append", default=[],
                        help="development aid: check only baseline paths containing this text")
    args = parser.parse_args()
    root = Path(args.repo_root).resolve()
    try:
        return audit(args.base, root / args.bundle, root / args.scope, root, tuple(args.only))
    except (OSError, ValueError, subprocess.CalledProcessError) as exc:
        print(f"coverage audit error: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
