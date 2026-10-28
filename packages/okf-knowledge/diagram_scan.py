#!/usr/bin/env python3
"""Detect likely authored ASCII/Unicode diagrams in repository documents.

The scanner intentionally reports candidates, not verdicts. The audit ledger
must classify each candidate as a converted diagram or a narrow exact-block
exception (for literal terminal output, code/configuration, or another
non-diagram use). A file-wide or directory-wide suppression is forbidden.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

DOC_SUFFIXES = {".md", ".mdx", ".rst", ".adoc", ".asciidoc"}
BOX = set("┌┐└┘├┤┬┴┼│─━┏┓┗┛╔╗╚╝║═╠╣╦╩╬╭╮╰╯")
TREE_LINE = re.compile(r"^\s*(?:├──|└──|│\s+├──|│\s+└──)")
ASCII_BOX_LINE = re.compile(r"^\s*\+[-=+| ]{3,}\+\s*$")
ASCII_LIFELINE = re.compile(r"^\s*[│|].*(?:[-=]{2,}>|<[-=]{2,})")
ARROW = re.compile(r"(?:-->|<--|<->|==>|->|<-|→|←|↔|⟶|⟵|⇒)")
TIMEPOINT = re.compile(r"^\s*T\+\d+(?:-\d+)?(?:ms|s|m|h)\s+")
FENCE = re.compile(r"^\s*(`{3,}|~{3,})(.*)$")


@dataclass(frozen=True)
class Candidate:
    path: str
    heading: str
    start: int
    end: int
    content: str
    context: str
    fenced: bool
    language: str


def is_shape_block(lines: list[str]) -> bool:
    visible = [line for line in lines if line.strip()]
    if not visible:
        return False
    unicode_box_lines = [line for line in visible if any(ch in BOX for ch in line)]
    tree_lines = [line for line in visible if TREE_LINE.match(line)]
    ascii_borders = [line for line in visible if ASCII_BOX_LINE.match(line)]
    ascii_lifelines = [line for line in visible if ASCII_LIFELINE.match(line)]
    if len(unicode_box_lines) >= 2 or len(tree_lines) >= 2:
        return True
    if len(ascii_borders) >= 2:
        return True
    if len(ascii_lifelines) >= 2:
        return True
    # Vertical flow sketches commonly use one connector line between each
    # state: `State`, `|`, `v`, `Next State`. Detect this without classifying
    # ordinary Markdown lists as diagrams.
    connectors = [line.strip() for line in visible]
    if sum(value in {"|", "v", "V", "↓", "\u2193"} for value in connectors) >= 2:
        return True
    if sum(bool(TIMEPOINT.match(line)) for line in visible) >= 3:
        return True
    # A text-only pipeline or state sketch often uses vertical branch bars and
    # several arrows. Require at least three graph-like lines to avoid flagging
    # ordinary prose, shell commands, or a single directional label.
    graph_lines = [line for line in visible if ARROW.search(line)]
    if len(graph_lines) >= 2:
        return True
    return len(graph_lines) >= 1 and any("|" in line or "+" in line for line in visible)


def scan_text(path: str, text: str) -> list[Candidate]:
    lines = text.splitlines()
    headings: list[tuple[int, str]] = []
    for n, line in enumerate(lines, 1):
        match = re.match(r"^\s*#{1,6}\s+(.*?)\s*#*\s*$", line)
        if match:
            headings.append((n, match.group(1)))

    candidates: list[Candidate] = []
    active_heading = "(document preamble)"
    heading_index = 0
    i = 0
    while i < len(lines):
        while heading_index < len(headings) and headings[heading_index][0] <= i + 1:
            active_heading = headings[heading_index][1]
            heading_index += 1
        fence = FENCE.match(lines[i])
        if fence:
            marker = fence.group(1)
            language = fence.group(2).strip().split(maxsplit=1)[0] if fence.group(2).strip() else ""
            start = i
            i += 1
            block_start = i
            while i < len(lines) and not re.match(rf"^\s*{re.escape(marker[0])}{{{len(marker)},}}\s*$", lines[i]):
                i += 1
            content = "\n".join(lines[block_start:i])
            # Tagged source languages are literal syntax, not presentation
            # diagrams. Plain text, ASCII and Markdown fences are scanned;
            # Markdown fences remain literal only when they present Markdown
            # source rather than an actual ASCII/Unicode figure.
            if language.lower() in {"", "text", "plaintext", "ascii"} and is_shape_block(content.splitlines()):
                candidates.append(Candidate(path, active_heading, start + 1, min(i + 1, len(lines)), content,
                                            "fenced code block", True, language))
            i += 1
            continue
        # Detect a contiguous unfenced figure. Do not inspect heading text or
        # normal prose rows as a standalone candidate.
        if any(ch in BOX for ch in lines[i]) or TREE_LINE.match(lines[i]):
            start = i
            block = [lines[i]]
            i += 1
            while i < len(lines) and (any(ch in BOX for ch in lines[i]) or TREE_LINE.match(lines[i]) or not lines[i].strip()):
                block.append(lines[i])
                i += 1
            if is_shape_block(block):
                candidates.append(Candidate(path, active_heading, start + 1, i, "\n".join(block),
                                            "unfenced figure", False, ""))
            continue
        # Compact relationship/pipeline diagrams are sometimes authored as a
        # single unfenced line. Require two explicit arrows and a short label
        # line rather than treating ordinary prose references as diagrams.
        line = lines[i].strip()
        if len(ARROW.findall(line)) >= 2 and len(line) <= 180 and not re.search(r"[.!?]$", line):
            candidates.append(Candidate(path, active_heading, i + 1, i + 1, line,
                                        "unfenced relationship/pipeline line", False, ""))
        i += 1
    return candidates


def git_paths(root: Path, rev: str | None = None) -> list[str]:
    if rev is None:
        return sorted(
            path.relative_to(root).as_posix()
            for path in root.rglob("*")
            if path.is_file()
            and path.suffix.lower() in DOC_SUFFIXES
            and not any(part in {".git", "node_modules", "target", "result", ".direnv"} for part in path.relative_to(root).parts)
        )
    data = subprocess.run(
        ["git", "ls-tree", "-r", "--name-only", "-z", rev],
        cwd=root,
        check=True,
        capture_output=True,
    ).stdout
    return [p.decode("utf-8", "surrogateescape") for p in data.split(b"\0") if p and Path(p.decode("utf-8", "surrogateescape")).suffix.lower() in DOC_SUFFIXES]


def read_revision(root: Path, path: str, rev: str | None) -> str:
    if rev is None:
        return (root / path).read_text(encoding="utf-8")
    return subprocess.run(["git", "show", f"{rev}:{path}"], cwd=root, check=True, capture_output=True, text=True).stdout


def candidate_key(c: Candidate) -> tuple[str, str]:
    """Returns the exception key: the path and a hash of the candidate text.

    Line numbers are excluded on purpose. Editing text above a classified
    candidate must not invalidate its exception. Editing the candidate itself
    must invalidate it, so a reviewer classifies the new text.
    """
    digest = hashlib.sha256(c.content.encode("utf-8")).hexdigest()
    return (c.path, digest)


def load_exceptions(path: Path) -> dict[tuple[str, str], int]:
    """Loads the exception ledger as a map of (path, hash) to allowed count.

    Every row MUST name one candidate and give a non-empty reason. A row never
    covers a file or a directory.
    """
    allowed: dict[tuple[str, str], int] = {}
    with path.open(encoding="utf-8", newline="") as handle:
        for row in csv.DictReader(handle, delimiter="\t"):
            if not row["reason"].strip():
                raise ValueError(f"exception without a reason: {row['path']} {row['content_sha256']}")
            key = (row["path"], row["content_sha256"])
            allowed[key] = allowed.get(key, 0) + int(row.get("count") or 1)
    return allowed


def check_against_exceptions(found: list[Candidate], allowed: dict[tuple[str, str], int]) -> tuple[list[Candidate], list[tuple[str, str]]]:
    """Returns unclassified candidates and stale exception keys."""
    remaining = dict(allowed)
    unclassified: list[Candidate] = []
    for c in found:
        key = candidate_key(c)
        if remaining.get(key, 0) > 0:
            remaining[key] -= 1
        else:
            unclassified.append(c)
    stale = [key for key, count in remaining.items() if count > 0]
    return unclassified, stale


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--revision", help="scan every document from this Git revision")
    parser.add_argument("--repo-root", default=".")
    parser.add_argument("--format", choices=("text", "tsv"), default="text")
    parser.add_argument("--exceptions", help="exception ledger TSV (path, content_sha256, count, reason)")
    parser.add_argument("--check", action="store_true",
                        help="exit 1 when a candidate has no exception row or an exception row matches nothing")
    args = parser.parse_args()
    root = Path(args.repo_root).resolve()
    paths = git_paths(root, args.revision)
    found: list[Candidate] = []
    for rel in paths:
        try:
            found.extend(scan_text(rel, read_revision(root, rel, args.revision)))
        except (OSError, subprocess.CalledProcessError, UnicodeDecodeError) as exc:
            print(f"cannot inspect {rel}: {exc}", file=sys.stderr)
            return 2
    if args.check:
        if not args.exceptions:
            print("--check requires --exceptions", file=sys.stderr)
            return 2
        unclassified, stale = check_against_exceptions(found, load_exceptions(Path(args.exceptions)))
        for c in unclassified:
            print(f"UNCLASSIFIED {c.path}:{c.start}-{c.end} [{c.heading}] {c.content[:120]!r}", file=sys.stderr)
        for path, digest in stale:
            print(f"STALE EXCEPTION {path} {digest}", file=sys.stderr)
        print(f"CANDIDATES={len(found)} UNCLASSIFIED={len(unclassified)} STALE={len(stale)}", file=sys.stderr)
        return 1 if unclassified or stale else 0
    if args.format == "tsv":
        print("path\theading\tstart\tend\tfenced\tlanguage\tcontent")
        for c in found:
            print("\t".join((c.path, c.heading.replace("\t", " "), str(c.start), str(c.end),
                             str(c.fenced).lower(), c.language, c.content.replace("\t", " ").replace("\n", "\\n"))))
    else:
        for c in found:
            print(f"{c.path}:{c.start}-{c.end} [{c.heading}] ({c.context}; lang={c.language or 'none'})")
            for line in c.content.splitlines():
                print(f"    {line}")
    print(f"CANDIDATES={len(found)}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
