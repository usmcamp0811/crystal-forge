#!/usr/bin/env python3
"""Line-level content coverage audit for migrated documents.

For each migrated source document, reads the original from Git history and
checks that every normalized, non-trivial line still occurs somewhere in the
union of its destination files. The check ignores heading levels, link
targets, blockquote markers, whitespace, code-fence markers, rules, and table
separator rows, because migration legitimately changes those.

This tool needs Git history, so it is a maintenance command. It is not part of
the hermetic flake check. Run it from the repository root:

    coverage-audit.py --base <commit> --source docs/x.md --dest a.md b.md
    coverage-audit.py --base <commit> --manifest

`--manifest` audits every moved, split, merged, or replaced source listed in
docs/knowledge/meta/migration-manifest/*.md. A source passes when 100 percent
of its normalized lines are found. Lines that are intentionally absent must be
listed in docs/knowledge/meta/migration-manifest/coverage-exceptions.txt as
`<source path>\t<normalized line>` with a reason on the following `#` line.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

LINK_TARGET = re.compile(r"\]\([^)]*\)")
FENCE_ONLY = re.compile(r"^(```|~~~)[\w-]*$")
RULE = re.compile(r"^([-*_]\s*){3,}$")
TABLE_SEP = re.compile(r"^\|?[\s:|-]+\|?$")


def normalize(line: str) -> str | None:
    text = LINK_TARGET.sub("]()", line)
    text = re.sub(r"^[\s#>]+", "", text)
    text = re.sub(r"\s+", " ", text).strip()
    if not text or FENCE_ONLY.match(text) or RULE.match(text) or TABLE_SEP.match(text):
        return None
    return text


def normalized_lines(text: str) -> list[str]:
    out = []
    for line in text.splitlines():
        value = normalize(line)
        if value is not None:
            out.append(value)
    return out


def strip_frontmatter(text: str) -> str:
    if text.startswith("---\n"):
        end = text.find("\n---\n", 4)
        if end != -1:
            return text[end + 5 :]
    return text


def git_show(base: str, path: str) -> str:
    return subprocess.run(["git", "show", f"{base}:{path}"], check=True,
                          capture_output=True, text=True).stdout


def audit(base: str, source: str, dests: list[Path], exceptions: set[str]) -> tuple[int, list[str]]:
    src_lines = normalized_lines(strip_frontmatter(git_show(base, source)))
    dest_set: set[str] = set()
    for dest in dests:
        dest_set.update(normalized_lines(strip_frontmatter(dest.read_text(encoding="utf-8"))))
    missing = [ln for ln in src_lines if ln not in dest_set and ln not in exceptions]
    return len(src_lines), missing


def load_exceptions(path: Path) -> dict[str, set[str]]:
    result: dict[str, set[str]] = {}
    if not path.exists():
        return result
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        source, _, text = line.partition("\t")
        result.setdefault(source, set()).add(text)
    return result


def manifest_sources(bundle: Path):
    manifest_dir = bundle / "meta" / "migration-manifest"
    for group in sorted(manifest_dir.glob("*.md")):
        if group.name in {"index.md", "log.md"}:
            continue
        in_table = False
        for line in group.read_text(encoding="utf-8").splitlines():
            if re.match(r"^\|\s*Original\s*\|", line):
                in_table = True
                continue
            if not in_table:
                continue
            if not line.startswith("|"):
                in_table = False
                continue
            if re.match(r"^\|[\s:|-]+\|$", line):
                continue
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            if len(cells) != 4:
                continue
            original, dest, action, _cov = cells
            if action not in {"moved", "split", "merged", "replaced"}:
                continue
            targets = [(group.parent / t.split("#")[0]).resolve()
                       for t in re.findall(r"\]\(([^)\s]+)\)", dest)
                       if not re.match(r"^[a-zA-Z][a-zA-Z0-9+.-]*:", t)]
            yield original.strip("`"), action, targets


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--base", required=True, help="commit that holds the original documents")
    parser.add_argument("--source")
    parser.add_argument("--dest", nargs="*", default=[])
    parser.add_argument("--manifest", action="store_true")
    parser.add_argument("--bundle", default="docs/knowledge")
    parser.add_argument("--show", type=int, default=15, help="max missing lines to print per source")
    args = parser.parse_args()

    bundle = Path(args.bundle).resolve()
    exceptions = load_exceptions(bundle / "meta" / "migration-manifest" / "coverage-exceptions.txt")
    failed = 0
    if args.manifest:
        rows = list(manifest_sources(bundle))
        for source, action, dests in rows:
            total, missing = audit(args.base, source, dests, exceptions.get(source, set()))
            pct = 100.0 * (total - len(missing)) / total if total else 100.0
            print(f"{pct:6.2f}%  {len(missing):4d} missing of {total:5d}  {action:8s} {source}")
            for line in missing[: args.show]:
                print(f"           - {line[:140]}")
            failed += 1 if missing else 0
        print(f"\n{len(rows)} source(s) audited, {failed} with missing lines")
    else:
        total, missing = audit(args.base, args.source, [Path(d).resolve() for d in args.dest],
                               exceptions.get(args.source, set()))
        pct = 100.0 * (total - len(missing)) / total if total else 100.0
        print(f"{pct:.2f}% covered, {len(missing)} missing of {total} normalized lines")
        for line in missing[: args.show]:
            print(f"  - {line[:160]}")
        failed = 1 if missing else 0
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
