#!/usr/bin/env python3
"""Validates the Crystal Forge OKF knowledge bundle in docs/knowledge/.

The validator enforces the structural rules of OKF v0.2 (frontmatter, required
`type`, reserved filenames, root `okf_version`) and the Crystal Forge
conventions recorded in docs/knowledge/meta/okf-conventions.md (type taxonomy,
`implementation_status`, provenance shape, relative links, index coverage,
migration manifest completeness).

It uses only the Python standard library and PyYAML. It never modifies files.

Usage:
    validate.py [--repo-root DIR] [--bundle PATH] [--skip-manifest]
                [--files FILE ...]

`--files` limits per-concept checks (frontmatter, links) to the listed files.
Index coverage and manifest checks always cover the whole bundle unless
`--skip-manifest` is given (which skips only the manifest checks).
"""

from __future__ import annotations

import argparse
import datetime as dt
import fnmatch
import hashlib
import os
import re
import sys
from urllib.parse import unquote, urlsplit
from pathlib import Path

import yaml

OKF_VERSION = "0.2"
RESERVED = {"index.md", "log.md"}
IMPLEMENTATION_STATUS = {"implemented", "partial", "proposed", "historical"}
LIFECYCLE_STATUS = {"draft", "stable", "deprecated"}
MANIFEST_ACTIONS = {"moved", "split", "merged", "retained", "replaced", "excluded"}
# Directories whose concepts do not need implementation_status.
STATUS_EXEMPT_DIRS = {"references", "meta"}
SOURCE_DESCRIPTOR = re.compile(
    r"^Crystal Forge repository file (\S+) at commit ([0-9a-f]{7,40})$"
)
ACTOR = re.compile(r"^(human:[^\s]+|process:[^\s]+|[^\s/:]+/[^\s]+)$")
LINK = re.compile(r"(!?)\[([^\]]*)\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")
FENCE = re.compile(r"^\s*(```|~~~)")
ABSOLUTE_LOCAL = re.compile(
    r"(?<![\w.~-])/(?:home|Users)/[A-Za-z0-9._-]+/|file:///(?:home|Users)/|(?<![\w.])C:\\\\Users"
)
# Directories scanned for source documents that must appear in the manifest.
MANIFEST_SKIP_PREFIXES = (
    ".git/", "node_modules/", "target/", "result/",
    "backlog/archive/", "backlog/completed/", "backlog/tasks/",
    "backlog/decisions/", "backlog/milestones/", "backlog/.backlog-templates/",
)


class Report:
    def __init__(self) -> None:
        self.errors: list[str] = []

    def error(self, path: str | Path, message: str) -> None:
        self.errors.append(f"{path}: {message}")


def slugify(heading: str) -> str:
    """Returns GitLab Flavored Markdown's documented heading slug."""
    text = heading.strip().lower()
    text = re.sub(r"[^\w\- ]", "", text, flags=re.UNICODE)
    return text.replace(" ", "-")


def heading_slugs(text: str) -> set[str]:
    slugs: set[str] = set()
    counts: dict[str, int] = {}
    in_fence = False
    for line in text.splitlines():
        if FENCE.match(line):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        match = re.match(r"^#{1,6}\s+(.*?)\s*#*\s*$", line)
        if not match:
            continue
        slug = slugify(match.group(1))
        n = counts.get(slug, 0)
        counts[slug] = n + 1
        slugs.add(slug if n == 0 else f"{slug}-{n}")
    return slugs


def split_frontmatter(text: str) -> tuple[str | None, str]:
    """Returns (frontmatter_text, body). frontmatter_text is None if absent."""
    if not text.startswith("---\n"):
        return None, text
    end = text.find("\n---\n", 4)
    if end == -1:
        if text.rstrip().endswith("\n---"):
            end = text.rstrip().rfind("\n---")
            return text[4:end], ""
        return None, text
    return text[4:end], text[end + 5 :]


def parse_taxonomy(conventions: Path) -> set[str]:
    text = conventions.read_text(encoding="utf-8")
    section = re.search(r"## Type taxonomy\n(.*?)(?:\n## |\Z)", text, re.S)
    if not section:
        raise SystemExit(f"{conventions}: missing '## Type taxonomy' section")
    return set(re.findall(r"^\| `([^`]+)` \|", section.group(1), re.M))


def iter_links(text: str):
    """Yields inline, reference, image, and HTML resource links outside code."""
    definitions: dict[str, str] = {}
    visible_lines: list[str] = []
    in_fence = False
    for lineno, line in enumerate(text.splitlines(), 1):
        # Markdown blockquotes may contain fenced code. Remove quote markers
        # before fence detection so quoted HTML examples remain literal code.
        line = re.sub(r"^\s*(?:>\s*)+", "", line)
        if FENCE.match(line):
            in_fence = not in_fence
            visible_lines.append("")
            continue
        if in_fence:
            visible_lines.append("")
            continue
        definition = re.match(r"^\s*\[([^]]+)\]:\s*(\S+)", line)
        if definition:
            definitions[definition.group(1).casefold()] = definition.group(2).strip("<>")
            visible_lines.append("")
        else:
            visible_lines.append(line)
    for lineno, line in enumerate(visible_lines, 1):
        stripped = re.sub(r"`+[^`]*`+", "", line)
        for match in LINK.finditer(stripped):
            yield bool(match.group(1)), match.group(2), match.group(3), lineno
        # Reference links and shortcut references.
        for match in re.finditer(r"(!?)\[([^]]+)\](?:\[([^]]*)\])?", stripped):
            label = match.group(3) or match.group(2)
            target = definitions.get(label.casefold())
            if target:
                yield bool(match.group(1)), match.group(2), target, lineno
        for match in re.finditer(r"<(?:img|source|a)\b[^>]*?\b(src|href)\s*=\s*['\"]([^'\"]+)['\"][^>]*>", stripped, re.I):
            yield match.group(1).lower() == "src", "HTML resource", match.group(2), lineno


def check_timestamp(report: Report, path: Path, field: str, value) -> None:
    if not isinstance(value, dt.datetime) or value.tzinfo is None:
        report.error(path, f"{field} must be an ISO 8601 datetime with explicit offset, got {value!r}")


def check_actor_event(report: Report, path: Path, field: str, event) -> None:
    if not isinstance(event, dict):
        report.error(path, f"{field} entries must be mappings with by/at")
        return
    by = event.get("by")
    if not isinstance(by, str) or not ACTOR.match(by):
        report.error(path, f"{field}.by must follow the actor convention, got {by!r}")
    if "at" in event:
        check_timestamp(report, path, f"{field}.at", event["at"])
    elif field == "verified":
        report.error(path, "verified entries require `at`")


def check_concept(report: Report, path: Path, rel: Path, taxonomy: set[str],
                  repo_root: Path) -> dict | None:
    text = path.read_text(encoding="utf-8")
    fm_text, _body = split_frontmatter(text)
    if fm_text is None:
        report.error(rel, "missing YAML frontmatter block")
        return None
    try:
        fm = yaml.safe_load(fm_text)
    except yaml.YAMLError as exc:
        report.error(rel, f"invalid YAML frontmatter: {exc}")
        return None
    if not isinstance(fm, dict):
        report.error(rel, "frontmatter must be a YAML mapping")
        return None

    type_ = fm.get("type")
    if not isinstance(type_, str) or not type_.strip():
        report.error(rel, "missing non-empty `type`")
    elif type_ not in taxonomy:
        report.error(rel, f"type {type_!r} is not in the Crystal Forge taxonomy")
    for key in ("title", "description"):
        value = fm.get(key)
        if not isinstance(value, str) or not value.strip():
            report.error(rel, f"missing non-empty `{key}`")
        elif "\n" in value.strip():
            report.error(rel, f"`{key}` must be a single line")
    tags = fm.get("tags")
    if not isinstance(tags, list) or "crystal-forge" not in tags:
        report.error(rel, "`tags` must be a list that includes crystal-forge")

    top_dir = rel.parts[0] if len(rel.parts) > 1 else ""
    impl = fm.get("implementation_status")
    if top_dir not in STATUS_EXEMPT_DIRS and type_ != "Reference":
        if impl not in IMPLEMENTATION_STATUS:
            report.error(rel, f"implementation_status must be one of {sorted(IMPLEMENTATION_STATUS)}, got {impl!r}")
    elif impl is not None and impl not in IMPLEMENTATION_STATUS:
        report.error(rel, f"invalid implementation_status {impl!r}")
    status = fm.get("status")
    if status is not None and status not in LIFECYCLE_STATUS:
        report.error(rel, f"status must be one of {sorted(LIFECYCLE_STATUS)}, got {status!r}")
    if impl == "historical" and status != "deprecated":
        report.error(rel, "implementation_status: historical requires status: deprecated")
    if status == "deprecated" and impl not in (None, "historical"):
        report.error(rel, "status: deprecated is reserved for historical content")

    generated = fm.get("generated")
    if generated is not None:
        if not isinstance(generated, dict) or "by" not in generated:
            report.error(rel, "generated must be a mapping with `by`")
        else:
            check_actor_event(report, rel, "generated", generated)
    verified = fm.get("verified")
    if verified is not None:
        events = [verified] if isinstance(verified, dict) else verified
        if not isinstance(events, list):
            report.error(rel, "verified must be a mapping or list of mappings")
        else:
            for event in events:
                check_actor_event(report, rel, "verified", event)
    stale = fm.get("stale_after")
    if stale is not None:
        check_timestamp(report, rel, "stale_after", stale)

    sources = fm.get("sources")
    if sources is not None:
        if not isinstance(sources, list) or not sources:
            report.error(rel, "sources must be a non-empty list when present")
        else:
            ids: set[str] = set()
            for entry in sources:
                if not isinstance(entry, dict) or not isinstance(entry.get("resource"), str):
                    report.error(rel, "each source needs a string `resource`")
                    continue
                resource = entry["resource"]
                match = SOURCE_DESCRIPTOR.match(resource)
                if not (match or resource.startswith(("http://", "https://"))):
                    report.error(rel, f"source resource {resource!r} must use the repository-file descriptor or an https URL")
                elif match:
                    source_path, source_rev = match.groups()
                    decoded = unquote(source_path)
                    if not re.fullmatch(r"[0-9a-f]{7,40}", source_rev):
                        report.error(rel, f"source resource {resource!r} has an invalid commit prefix")
                    if decoded.startswith("/") or ".." in Path(decoded).parts:
                        report.error(rel, f"source resource path {decoded!r} is not repository-relative")
                    candidate = (repo_root / decoded).resolve()
                    try:
                        candidate.relative_to(repo_root.resolve())
                    except ValueError:
                        report.error(rel, f"source resource {resource!r} leaves the repository")
                    else:
                        if Path(decoded).suffix.lower() in {".md", ".mdx", ".rst", ".adoc", ".asciidoc"}:
                            inventory = repo_root / "docs/knowledge/meta/migration-manifest/source-inventory.tsv"
                            if inventory.is_file():
                                inventory_paths = {
                                    line.split("\t", 1)[0]
                                    for line in inventory.read_text(encoding="utf-8").splitlines()[1:]
                                    if line and not line.startswith("#")
                                }
                                if decoded not in inventory_paths:
                                    report.error(rel, f"source resource path {decoded!r} is absent from source inventory")
                sid = entry.get("id")
                if sid is not None:
                    if sid in ids:
                        report.error(rel, f"duplicate source id {sid!r}")
                    ids.add(sid)
                if "last_modified" in entry:
                    check_timestamp(report, rel, "sources.last_modified", entry["last_modified"])
    return fm


def check_links(report: Report, path: Path, rel: Path, repo_root: Path,
                slug_cache: dict[Path, set[str]], text: str | None = None) -> None:
    if text is None:
        text = path.read_text(encoding="utf-8")
    for is_image, _label, target, lineno in iter_links(text):
        if re.match(r"^[a-zA-Z][a-zA-Z0-9+.-]*:", target):
            if target.startswith("file:"):
                report.error(f"{rel}:{lineno}", f"file: link {target!r}")
            continue
        if target.startswith("#"):
            anchor_file, fragment = path, target[1:]
        else:
            if target.startswith("/"):
                report.error(f"{rel}:{lineno}", f"absolute link {target!r}; use a relative link")
                continue
            parsed = urlsplit(target)
            file_part, fragment = unquote(parsed.path), unquote(parsed.fragment)
            anchor_file = (path.parent / file_part).resolve()
            try:
                anchor_file.relative_to(repo_root.resolve())
            except ValueError:
                report.error(f"{rel}:{lineno}", f"link {target!r} leaves the repository")
                continue
            if not anchor_file.exists():
                kind = "image" if is_image else "link"
                report.error(f"{rel}:{lineno}", f"broken {kind} target {target!r}")
                continue
        fragment = unquote(fragment)
        if fragment and anchor_file.is_file() and anchor_file.suffix == ".md":
            slugs = slug_cache.get(anchor_file)
            if slugs is None:
                slugs = heading_slugs(anchor_file.read_text(encoding="utf-8"))
                slug_cache[anchor_file] = slugs
            if fragment not in slugs:
                report.error(f"{rel}:{lineno}", f"anchor #{fragment} not found in {anchor_file.name}")
    for lineno, line in enumerate(text.splitlines(), 1):
        if ABSOLUTE_LOCAL.search(line):
            report.error(f"{rel}:{lineno}", "absolute local filesystem path")


def check_index(report: Report, path: Path, rel: Path, bundle: Path, is_root: bool) -> None:
    text = path.read_text(encoding="utf-8")
    fm_text, body = split_frontmatter(text)
    if is_root:
        if fm_text is None:
            report.error(rel, f'root index must declare okf_version: "{OKF_VERSION}"')
        else:
            fm = yaml.safe_load(fm_text) or {}
            if fm != {"okf_version": OKF_VERSION}:
                report.error(rel, f'root index frontmatter must be exactly okf_version: "{OKF_VERSION}", got {fm!r}')
    elif fm_text is not None:
        report.error(rel, "index files must not carry frontmatter")
    directory = path.parent
    entry = re.compile(r"^\s*[*-]\s+\[([^\]]+)\]\(([^)\s]+)\)\s+-\s+(\S.*)$")
    listed: set[Path] = set()
    for lineno, line in enumerate(body.splitlines(), 1):
        if re.match(r"^\s*[*-]\s+\[", line):
            match = entry.match(line)
            if not match:
                report.error(f"{rel}:{lineno}", "index entry must be `* [Title](link) - description`")
                continue
            target = match.group(2).split("#")[0]
            if re.match(r"^[a-zA-Z][a-zA-Z0-9+.-]*:", target):
                continue
            resolved = (directory / target).resolve()
            listed.add(resolved)
            if resolved.is_dir():
                listed.add((resolved / "index.md").resolve())
    expected: list[Path] = []
    for child in sorted(directory.iterdir()):
        if child.is_file() and child.suffix == ".md" and child.name not in RESERVED:
            expected.append(child.resolve())
        elif child.is_dir() and not child.name.startswith("."):
            expected.append((child / "index.md").resolve())
    for item in expected:
        if item not in listed:
            report.error(rel, f"index does not list {item.relative_to(directory.resolve())}")


def check_log(report: Report, path: Path, rel: Path) -> None:
    text = path.read_text(encoding="utf-8")
    if split_frontmatter(text)[0] is not None:
        report.error(rel, "log files must not carry frontmatter")
    dates = re.findall(r"^## (.+)$", text, re.M)
    for value in dates:
        if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", value):
            report.error(rel, f"log date heading {value!r} must be YYYY-MM-DD")
    if dates != sorted(dates, reverse=True):
        report.error(rel, "log entries must be newest first")


def parse_manifest_rows(path: Path):
    """Yields (lineno, original, destination_cell, action, coverage)."""
    lines = path.read_text(encoding="utf-8").splitlines()
    in_table = False
    for lineno, line in enumerate(lines, 1):
        if re.match(r"^\|\s*Original\s*\|\s*Destination\s*\|\s*Action\s*\|\s*Coverage\s*\|", line):
            in_table = True
            continue
        if in_table:
            if not line.startswith("|"):
                in_table = False
                continue
            if re.match(r"^\|[\s:|-]+\|$", line):
                continue
            cells = [c.strip() for c in re.split(r"(?<!\\)\|", line.strip().strip("|"))]
            if len(cells) != 4:
                yield lineno, None, line, None, None
                continue
            yield lineno, cells[0].strip("`"), cells[1], cells[2], cells[3]


def check_manifest(report: Report, bundle: Path, repo_root: Path) -> None:
    manifest_dir = bundle / "meta" / "migration-manifest"
    if not manifest_dir.is_dir():
        report.error("meta/migration-manifest", "missing manifest directory")
        return
    seen: dict[str, Path] = {}
    patterns: list[tuple[str, str]] = []
    for group in sorted(manifest_dir.glob("*.md")):
        if group.name in RESERVED:
            continue
        rel = group.relative_to(bundle)
        rows = 0
        for lineno, original, dest, action, coverage in parse_manifest_rows(group):
            where = f"{rel}:{lineno}"
            rows += 1
            if original is None:
                report.error(where, "malformed manifest row")
                continue
            if original in seen:
                report.error(where, f"{original} also appears in {seen[original].name}")
            seen[original] = group
            if action not in MANIFEST_ACTIONS:
                report.error(where, f"action {action!r} not in {sorted(MANIFEST_ACTIONS)}")
            if coverage != "complete":
                report.error(where, f"{original}: Coverage must be `complete`, got {coverage!r}")
            exists = (repo_root / original).exists() if "*" not in original else bool(
                [p for p in repo_root.glob(original)])
            if action in {"retained", "excluded", "replaced"}:
                if not exists:
                    report.error(where, f"{original} is {action} but does not exist")
            elif action in {"moved", "split", "merged"}:
                if exists:
                    report.error(where, f"{original} is {action} but still exists")
            if "*" in original:
                patterns.append((original, str(group)))
            targets = re.findall(r"\]\(([^)\s]+)\)", dest)
            if action in {"moved", "split", "merged", "replaced"} and not targets:
                report.error(where, f"{original}: destination column needs links")
            if action in {"retained", "excluded"} and "(" not in dest and not dest:
                report.error(where, f"{original}: destination column is empty")
            for target in targets:
                if re.match(r"^[a-zA-Z][a-zA-Z0-9+.-]*:", target):
                    continue
                resolved = (group.parent / target.split("#")[0]).resolve()
                if not resolved.exists():
                    report.error(where, f"{original}: destination {target!r} does not exist")
                elif action in {"moved", "split", "merged"}:
                    try:
                        resolved.relative_to(bundle.resolve())
                    except ValueError:
                        report.error(where, f"{original}: destination {target!r} is outside the bundle")
        if rows == 0:
            report.error(rel, "manifest group file has no table rows")
    # Every Markdown-like source outside the bundle needs a manifest entry.
    doc_suffixes = {".md", ".mdx", ".rst", ".adoc"}
    bundle_resolved = bundle.resolve()
    # Files explicitly kept outside the bundle (per okf-conventions.md)
    ROOT_KEPT = {
        "AGENTS.md", "CLAUDE.md", "CLA.md", "README.md",
        "CONTRIBUTING.md", "FRONTEND_TODO.md", "ROADMAP.md", "TESTING.md",
    }
    for root, dirs, files in os.walk(repo_root):
        rel_root = Path(root).relative_to(repo_root)
        dirs[:] = [d for d in dirs if not d.startswith(".git") and d not in {"node_modules", "target", "result"}]
        for name in files:
            if Path(name).suffix not in doc_suffixes:
                continue
            rel_path = (rel_root / name).as_posix()
            if any(rel_path.startswith(p) for p in MANIFEST_SKIP_PREFIXES):
                continue
            # Skip root-level files that are intentionally kept in place
            if rel_root == Path(".") and name in ROOT_KEPT:
                continue
            # Skip .gitlab templates
            if rel_path.startswith(".gitlab/"):
                continue
            # Skip agent-control files (per okf-conventions.md)
            if rel_path.startswith("docs/agents/"):
                continue
            # Skip design handoff uploads (per okf-conventions.md)
            if rel_path.startswith("docs/design/CrystalForge/uploads/"):
                continue
            if (Path(root) / name).resolve().is_relative_to(bundle_resolved):
                continue
            if rel_path in seen:
                continue
            if any(fnmatch.fnmatch(rel_path, pat) for pat, _ in patterns):
                continue
            report.error("meta/migration-manifest", f"{rel_path} has no manifest entry")


def check_duplicates(report: Report, concept_paths: list[Path], bundle: Path) -> None:
    digests: dict[str, Path] = {}
    for path in concept_paths:
        _fm, body = split_frontmatter(path.read_text(encoding="utf-8"))
        normalized = re.sub(r"\s+", " ", body).strip()
        if len(normalized) < 200:
            continue
        digest = hashlib.sha256(normalized.encode()).hexdigest()
        if digest in digests:
            report.error(path.relative_to(bundle), f"duplicate body of {digests[digest].relative_to(bundle)}")
        digests[digest] = path


def check_navigation_graph(report: Report, bundle: Path, repo_root: Path) -> None:
    """Require parsed Markdown links to make every concept reachable from root."""
    documents = {p.resolve() for p in bundle.rglob("*.md")}
    root_index = (bundle / "index.md").resolve()
    reached: set[Path] = set()
    pending = [root_index]
    while pending:
        source = pending.pop()
        if source in reached or not source.is_file():
            continue
        reached.add(source)
        for _image, _label, target, _line in iter_links(source.read_text(encoding="utf-8")):
            if re.match(r"^[A-Za-z][A-Za-z0-9+.-]*:", target):
                continue
            parsed = urlsplit(target)
            if parsed.path.startswith("/"):
                continue
            candidate = (source.parent / unquote(parsed.path)).resolve()
            if candidate.is_dir():
                candidate = candidate / "index.md"
            if candidate in documents and candidate not in reached:
                pending.append(candidate)
    for concept in documents:
        if concept.name not in RESERVED and concept not in reached:
            report.error(concept.relative_to(bundle), "concept is not reachable from the root index through parsed Markdown links")


def check_external_navigation(report: Report, repo_root: Path, bundle: Path,
                              slug_cache: dict[Path, set[str]]) -> None:
    """Validate links in retained entry points and README/pointer documents."""
    selected = {"README.md", "CONTRIBUTING.md", "AGENTS.md", "CLAUDE.md", "TESTING.md"}
    paths: set[Path] = {repo_root / name for name in selected}
    for pattern in ("checks/**/README.md", "packages/**/README.md", "docs/design/**/*.md"):
        paths.update(repo_root.glob(pattern))
    for path in repo_root.glob("backlog/docs/**/*.md"):
        text = path.read_text(encoding="utf-8")
        if "docs/knowledge/" in text or "knowledge/index.md" in text:
            paths.add(path)
    for path in sorted(p for p in paths if p.is_file() and not p.resolve().is_relative_to(bundle.resolve())):
        lines = path.read_text(encoding="utf-8").splitlines()
        # The retained-file contract concerns links into the migrated corpus.
        # Unrelated legacy links remain outside this migration validator.
        relevant = [line for line in lines if "docs/knowledge" in line or "knowledge/index" in line]
        if relevant:
            check_links(report, path, path.relative_to(repo_root), repo_root, slug_cache,
                        "\n".join(relevant))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--repo-root", default=".")
    parser.add_argument("--bundle", default="docs/knowledge")
    parser.add_argument("--skip-manifest", action="store_true")
    parser.add_argument("--files", nargs="*", default=None)
    args = parser.parse_args()

    repo_root = Path(args.repo_root).resolve()
    bundle = (repo_root / args.bundle).resolve()
    report = Report()
    if not bundle.is_dir():
        print(f"bundle {bundle} not found", file=sys.stderr)
        return 2
    taxonomy = parse_taxonomy(bundle / "meta" / "okf-conventions.md")
    limit = {Path(f).resolve() for f in args.files} if args.files is not None else None

    slug_cache: dict[Path, set[str]] = {}
    concept_paths: list[Path] = []
    for path in sorted(bundle.rglob("*.md")):
        rel = path.relative_to(bundle)
        selected = limit is None or path.resolve() in limit
        if path.name == "index.md":
            if limit is None:
                check_index(report, path, rel, bundle, path.parent == bundle)
            if selected:
                check_links(report, path, rel, repo_root, slug_cache)
        elif path.name == "log.md":
            if limit is None:
                check_log(report, path, rel)
            if selected:
                check_links(report, path, rel, repo_root, slug_cache)
        else:
            concept_paths.append(path)
            if selected:
                check_concept(report, path, rel, taxonomy, repo_root)
                check_links(report, path, rel, repo_root, slug_cache)
    # Every directory must have an index so the bundle stays navigable.
    if limit is None:
        for directory in sorted(p for p in bundle.rglob("*") if p.is_dir()):
            if not (directory / "index.md").is_file():
                report.error(directory.relative_to(bundle), "directory has no index.md")
        if not (bundle / "index.md").is_file():
            report.error(".", "missing root index.md")
        check_duplicates(report, concept_paths, bundle)
        check_navigation_graph(report, bundle, repo_root)
        check_external_navigation(report, repo_root, bundle, slug_cache)
        if not args.skip_manifest:
            check_manifest(report, bundle, repo_root)

    if report.errors:
        for line in report.errors:
            print(line)
        print(f"\nFAILED: {len(report.errors)} problem(s)", file=sys.stderr)
        return 1
    print(f"OK: {len(concept_paths)} concept(s) validated")
    return 0


if __name__ == "__main__":
    sys.exit(main())
