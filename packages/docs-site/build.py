"""Build the static Crystal Forge knowledge and generated reference site.

Adapted from Matthew Camp's dotfiles OKF renderer. See README.md for provenance
and the upstream license. Only explicitly supplied Markdown bundles and referenced
repository images are published. Repository source links remain GitLab links.
"""
from __future__ import annotations

import argparse
import html
import json
import os
import re
import shutil
from dataclasses import dataclass, replace
from html.parser import HTMLParser
from pathlib import Path, PurePosixPath
from urllib.parse import quote, unquote, urlsplit, urlunsplit

import yaml
from markdown_it import MarkdownIt
from mdit_py_plugins.anchors import anchors_plugin
from mdit_py_plugins.footnote import footnote_plugin
from pygments import highlight
from pygments.formatters import HtmlFormatter
from pygments.lexers import get_lexer_by_name
from pygments.util import ClassNotFound

ASSETS = Path(__file__).resolve().parent / "assets"
MARKER = ".okf-site"
IMAGE_TYPES = {".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp", ".avif"}


def esc(value) -> str:
    return html.escape(str(value), quote=True)


def relurl(source: PurePosixPath, target: PurePosixPath) -> str:
    return quote(Path(os.path.relpath(str(target), str(source.parent))).as_posix())


def code_highlight(code, language, attrs):
    try:
        lexer = get_lexer_by_name(language.split()[0])
    except (ClassNotFound, IndexError):
        return ""
    return highlight(code, lexer, HtmlFormatter(nowrap=True))


def markdown() -> MarkdownIt:
    # Raw HTML stays disabled: bundle prose cannot inject scripts or bypass the
    # link/image publication boundary through raw HTML attributes.
    md = MarkdownIt("commonmark", {"html": False, "highlight": code_highlight})
    md.enable(["table", "strikethrough"])
    md.use(footnote_plugin)
    md.use(anchors_plugin, min_level=1, max_level=6)
    return md


@dataclass
class Page:
    source: Path
    out: PurePosixPath
    bundle: str
    title: str
    meta: dict
    body: str


def read_page(source: Path, bundle: Path) -> Page:
    text = source.read_text(encoding="utf-8")
    match = re.match(r"\A---\r?\n(.*?)\r?\n---(?:\r?\n|$)", text, re.S)
    meta = yaml.safe_load(match[1]) if match else {}
    if meta is None:
        meta = {}
    if not isinstance(meta, dict):
        raise ValueError(f"{source}: frontmatter must be a mapping")
    body = text[match.end():] if match else text
    heading = re.search(r"^#\s+(.+)$", body, re.M)
    title = str(meta.get("title") or (heading[1] if heading else source.stem))
    return Page(source, PurePosixPath(bundle.name) / source.relative_to(bundle).with_suffix(".html"),
                bundle.name, title, meta, body)


class Site:
    """Render explicit bundles without copying their surrounding directories."""

    def __init__(self, bundles, repository_url, repository_root=None, revision="dev", mermaid=None):
        self.bundles = [Path(b).resolve() for b in bundles]
        if len({b.name for b in self.bundles}) != len(self.bundles):
            raise ValueError("bundle directory names must be unique")
        if not self.bundles or any(not b.is_dir() for b in self.bundles):
            raise ValueError("supply at least one existing bundle directory")
        self.repository_url = repository_url.rstrip("/")
        if urlsplit(self.repository_url).scheme not in {"http", "https"}:
            raise ValueError("repository URL must use HTTP or HTTPS")
        self.revision = revision
        self.mermaid = Path(mermaid) if mermaid else None
        # Generated bundles can live outside the checkout. Infer the root only
        # from a bundle with a repository ancestor; never use a common /nix/store
        # parent as an implicit publication boundary.
        self.repository_root = Path(repository_root).resolve() if repository_root else next(
            (p for b in self.bundles for p in (b, *b.parents) if (p / ".git").exists()), None)
        if self.repository_root is None:
            raise ValueError("cannot infer repository root; pass --repository-root")
        self.md = markdown()
        self.pages = []
        for bundle in self.bundles:
            for source in sorted(bundle.rglob("*.md")):
                if source.is_symlink() or not source.resolve().is_relative_to(bundle):
                    raise ValueError(f"bundle Markdown escapes its root: {source}")
                self.pages.append(read_page(source, bundle))
        self.by_source = {p.source: p for p in self.pages}
        self.assets = {}
        self._indexes()

    def _indexes(self):
        """Fill missing navigation only; authored index bodies remain canonical."""
        for bundle in self.bundles:
            members = [p for p in self.pages if p.bundle == bundle.name]
            directories = {bundle}
            for page in members:
                directories.update(p for p in page.source.parents if p.is_relative_to(bundle))
            for directory in sorted(directories):
                source = directory / "index.md"
                if source in self.by_source:
                    continue
                children = [p for p in members if p.source.parent == directory]
                links = [f"* [{p.title}]({quote(p.source.name)})" for p in children]
                subdirs = sorted(d for d in directories if d.parent == directory)
                links += [f"* [{d.name}]({quote(d.name)}/index.md)" for d in subdirs]
                page = Page(source, PurePosixPath(bundle.name) / source.relative_to(bundle).with_suffix(".html"),
                            bundle.name, directory.name, {}, f"# {directory.name}\n\n" + "\n".join(links))
                self.pages.append(page)
                self.by_source[source] = page

    def rewrite(self, page: Page, href: str) -> str:
        parts = urlsplit(href)
        if parts.scheme or parts.netloc or not parts.path:
            return href
        bare = unquote(parts.path)
        # Leading slashes in knowledge Markdown mean repository-root paths,
        # never deployment-root paths. All emitted site URLs are relative.
        target = (self.repository_root / bare.lstrip("/") if bare.startswith("/")
                  else page.source.parent / bare)
        target = target.resolve()
        found = self.by_source.get(target) or self.by_source.get(target / "index.md")
        if found:
            destination = relurl(page.out, found.out)
        else:
            safe_repository = target.is_relative_to(self.repository_root)
            safe_bundle = next((b for b in self.bundles if target.is_relative_to(b)), None)
            # Only referenced browser images cross the publication boundary.
            # resolve() above also rejects symlinks that lead outside safe roots.
            if target.is_file() and target.suffix.lower() in IMAGE_TYPES and (safe_repository or safe_bundle):
                if safe_repository:
                    output = PurePosixPath("repository-images") / target.relative_to(self.repository_root)
                else:
                    output = PurePosixPath(safe_bundle.name) / target.relative_to(safe_bundle)
                self.assets[output] = target
                destination = relurl(page.out, output)
            elif safe_repository:
                path = quote(target.relative_to(self.repository_root).as_posix())
                destination = f"{self.repository_url}/-/blob/{quote(self.revision, safe='')}/{path}"
            else:
                raise ValueError(f"{page.source}: reference escapes repository/bundles: {href}")
        return urlunsplit(("", "", destination, parts.query, parts.fragment))

    def render_body(self, page):
        env = {}
        tokens = self.md.parse(page.body, env)
        for token in tokens:
            for child in token.children or []:
                if child.type in {"link_open", "image"}:
                    key = "src" if child.type == "image" else "href"
                    child.attrSet(key, self.rewrite(page, child.attrGet(key) or ""))
        return self.md.renderer.render(tokens, self.md.options, env)

    def navigation(self, output, current=None):
        """Group every bundle by directory and expand the current document path."""
        def link(page):
            active = ' aria-current="page"' if current and current.source == page.source else ""
            label = "Overview" if page.source.name == "index.md" else page.title
            return f'<li><a href="{esc(relurl(output, page.out))}"{active}>{esc(label)}</a></li>'

        def directory(root, members):
            direct = sorted((p for p in members if p.source.parent == root),
                            key=lambda p: (p.source.name != "index.md", p.title.lower()))
            result = "<ul>" + "".join(link(p) for p in direct)
            children = sorted({root / p.source.relative_to(root).parts[0] for p in members
                               if p.source.parent != root})
            for child in children:
                opened = " open" if current and current.source.is_relative_to(child) else ""
                result += f'<li><details{opened}><summary>{esc(child.name.replace("-", " "))}</summary>'
                result += directory(child, [p for p in members if p.source.is_relative_to(child)]) + "</details></li>"
            return result + "</ul>"

        sections = []
        for bundle in self.bundles:
            label = {"knowledge": "Knowledge", "options": "NixOS options"}.get(bundle.name, bundle.name)
            members = [p for p in self.pages if p.bundle == bundle.name]
            sections.append(f'<section><h2>{esc(label)}</h2>{directory(bundle, members)}</section>')
        return "".join(sections)

    def shell(self, output, title, body, navigation="", search=False):
        up = "../" * (len(output.parts) - 1)
        bundles = " ".join(f'<a href="{esc(relurl(output, PurePosixPath(b.name) / "index.html"))}">{esc({"knowledge": "Knowledge", "options": "NixOS options"}.get(b.name, b.name))}</a>'
                           for b in self.bundles)
        return f'''<!doctype html>
<html lang="en" data-theme="dark"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{esc(title)} | Crystal Forge</title><script src="{up}assets/theme.js"></script>
<link rel="stylesheet" href="{up}assets/style.css">
<link rel="icon" type="image/png" href="{up}assets/cf-logo-transparent.png">
<script defer src="{up}assets/mermaid.min.js"></script>
{f'<script defer src="{up}assets/search-index.js"></script>' if search else ''}
<script defer src="{up}assets/app.js"></script></head><body>
<a class="skip" href="#main">Skip to content</a>
<header><a class="brand" href="{up}index.html" aria-label="Crystal Forge documentation home">
<img src="{up}assets/cf-logo-transparent.png" alt="" width="44" height="44">
<span>Crystal Forge<small>Documentation</small></span></a>
<nav aria-label="Bundles">{bundles}</nav><form action="{up}search.html" role="search">
<input aria-label="Search" type="search" name="q" placeholder="Search knowledge and options"><button>Search</button></form>
<button id="theme-toggle" type="button" aria-label="Light theme" aria-pressed="false" hidden>Light theme</button>
<button id="nav-toggle" type="button" aria-controls="document-nav" aria-expanded="false" hidden>Browse docs</button></header>
<div class="layout"><nav id="document-nav" class="side" aria-label="Documents">{navigation or self.navigation(output)}</nav>
<main id="main">{body}</main></div><footer>Built from canonical knowledge and generated reference bundles.
<a href="{esc(self.repository_url)}">Repository</a> · Open Knowledge Format 0.2 ·
<a href="{up}assets/renderer-attribution.txt">Renderer attribution and license</a></footer></body></html>'''

    def page_html(self, page):
        # Status is independent from verification/provenance. Missing metadata
        # must never look like an assertion that the behavior is implemented.
        status = str(page.meta.get("implementation_status", "unspecified"))
        metadata = (f'<section class="metadata" aria-label="Document metadata">'
                    f'<strong>Implementation status: <span class="status" data-status="{esc(status)}">{esc(status)}</span></strong>'
                    f'<p>{esc(page.meta.get("description", ""))}</p>'
                    f'<span>{esc(page.meta.get("type", "Index" if page.source.name == "index.md" else "Concept"))}</span></section>')
        body = self.render_body(page)
        if not re.search(r"<h1[ >]", body):
            body = f"<h1>{esc(page.title)}</h1>" + body
        crumb = (f'<nav class="breadcrumbs" aria-label="Breadcrumb"><a href="{esc(relurl(page.out, PurePosixPath(page.bundle) / "index.html"))}">'
                 f'{esc({"knowledge": "Knowledge", "options": "NixOS options"}.get(page.bundle, page.bundle))}</a>'
                 f'<span aria-hidden="true">/</span><span>{esc(page.title)}</span></nav>')
        return self.shell(page.out, page.title, crumb + metadata + '<article>' + body + '</article>',
                          self.navigation(page.out, page))

    def build(self, out: Path):
        """Replace an owned output, then fail if any local site target is absent."""
        out = Path(out).absolute()
        if any(b == out or b.is_relative_to(out) or out.is_relative_to(b) for b in self.bundles):
            raise ValueError("output must be separate from input bundles")
        if self.repository_root.is_relative_to(out.resolve()):
            raise ValueError("output must not contain the repository")
        if out.is_symlink():
            raise ValueError("output must not be a symlink")
        if out.exists():
            if any(out.iterdir()) and not (out / MARKER).is_file():
                raise ValueError(f"refusing to replace non-site directory: {out}")
            shutil.rmtree(out)
        (out / "assets").mkdir(parents=True)
        (out / MARKER).write_text("Crystal Forge OKF site\n", encoding="utf-8")

        def write(path, content):
            target = out / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(content, encoding="utf-8")

        for page in self.pages:
            write(page.out, self.page_html(page))
        # The home page is the canonical bundle entrance, rendered at its new
        # location so the curated links keep working under every Pages prefix.
        entrance = next(p for p in self.pages if p.bundle == self.bundles[0].name and p.source.name == "index.md"
                        and p.source.parent == self.bundles[0])
        write("index.html", self.page_html(replace(entrance, out=PurePosixPath("index.html"))))
        write("search.html", self.shell(PurePosixPath("search.html"), "Search",
              '<h1>Search</h1><p>Match all words, or filter with <code>status:proposed</code>, '
              '<code>bundle:knowledge</code>, or <code>type:Guide</code>.</p>'
              '<p id="result-count" role="status"></p><ol id="results"></ol>', search=True))
        records = [{"u": quote(p.out.as_posix()), "t": p.title, "b": p.bundle,
                    "s": str(p.meta.get("implementation_status", "unspecified")),
                    "y": str(p.meta.get("type", "Concept")), "x": p.body,
                    "d": str(p.meta.get("description", ""))} for p in self.pages]
        write("assets/search-index.js", "window.OKF_INDEX = " + json.dumps(records, ensure_ascii=True) + ";\n")
        for name in ("style.css", "app.js", "theme.js"):
            shutil.copyfile(ASSETS / name, out / "assets" / name)
        logo = self.repository_root / "docs/cf-logo-transparent.png"
        if not logo.resolve().is_relative_to(self.repository_root):
            raise ValueError("branding image escapes repository")
        shutil.copyfile(logo, out / "assets/cf-logo-transparent.png")
        shutil.copyfile(ASSETS.parent / "README.md", out / "assets/renderer-attribution.txt")
        if self.mermaid is None or not self.mermaid.is_file():
            raise ValueError("a bundled Mermaid JavaScript file is required")
        shutil.copyfile(self.mermaid, out / "assets/mermaid.min.js")
        for target, source in self.assets.items():
            (out / target).parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, out / target)
        problems = verify_site(out)
        if problems:
            raise ValueError("site verification failed:\n" + "\n".join(problems))
        return {"pages": len(self.pages), "images": len(self.assets), "problems": problems}


class References(HTMLParser):
    def __init__(self):
        super().__init__()
        self.refs = []

    def handle_starttag(self, tag, attrs):
        self.refs.extend(value for key, value in attrs if value and key in {"href", "src", "action"})


def verify_site(root: Path) -> list[str]:
    """Check local targets and reject links that escape a deployment prefix."""
    root = root.resolve()
    problems = []
    for page in sorted(root.rglob("*.html")):
        parser = References()
        parser.feed(page.read_text(encoding="utf-8"))
        for ref in parser.refs:
            parts = urlsplit(ref)
            if parts.scheme or parts.netloc or not parts.path:
                continue
            target = (page.parent / unquote(parts.path)).resolve()
            if parts.path.startswith("/") or not target.is_relative_to(root):
                problems.append(f"{page.relative_to(root)}: prefix-unsafe {ref}")
            elif not target.is_file():
                problems.append(f"{page.relative_to(root)}: missing {ref}")
    return problems


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--mermaid", required=True, type=Path)
    parser.add_argument("--repository-url", required=True)
    parser.add_argument("--repository-root", type=Path)
    parser.add_argument("--revision", default="dev")
    parser.add_argument("bundles", nargs="+", type=Path)
    args = parser.parse_args(argv)
    try:
        site = Site(args.bundles, args.repository_url, args.repository_root, args.revision, args.mermaid)
        print(json.dumps(site.build(args.out), sort_keys=True))
    except (ValueError, OSError, yaml.YAMLError) as error:
        parser.exit(1, f"site build: {error}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
