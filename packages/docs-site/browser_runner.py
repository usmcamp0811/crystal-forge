"""Serve immutable site inputs temporarily and check both deployment layouts.

Each server binds port zero on loopback and retains the socket until shutdown.
The prefixed server rejects paths outside its mount, so root-relative asset
requests cannot accidentally succeed against the root deployment. All browser
state and artifacts are written outside the site input.
"""

import argparse
from contextlib import ExitStack, contextmanager
from functools import partial
from html.parser import HTMLParser
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import subprocess
import sys
from threading import Thread
from urllib.parse import quote, urlsplit

from playwright.sync_api import expect, sync_playwright


class DiagramCounter(HTMLParser):
    """Count published Mermaid fences before JavaScript replaces them."""

    def __init__(self):
        super().__init__()
        self.count = 0

    def handle_starttag(self, tag, attrs):
        if tag == "code" and "language-mermaid" in dict(attrs).get("class", "").split():
            self.count += 1


class SiteHandler(SimpleHTTPRequestHandler):
    """Serve only the configured URL mount from the read-only site directory."""

    def __init__(self, *args, mount, **kwargs):
        self.mount = mount
        super().__init__(*args, **kwargs)

    def send_head(self):
        if not urlsplit(self.path).path.startswith(self.mount):
            self.send_error(404, "Outside documentation mount")
            return None
        # Keep the query string for the standard handler. Preserve a leading
        # slash so its directory redirects remain relative to this mount.
        self.path = "/" + self.path[len(self.mount):]
        return super().send_head()

    def log_message(self, format, *args):
        # Successful corpus requests are numerous; retain HTTP failure details.
        if len(args) > 1 and str(args[1]).isdigit() and int(args[1]) >= 400:
            super().log_message(format, *args)


@contextmanager
def serve(site, mount):
    """Keep a build-owned HTTP listener alive only while checks use it."""
    handler = partial(SiteHandler, directory=str(site), mount=mount)
    with ThreadingHTTPServer(("127.0.0.1", 0), handler) as server:
        thread = Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            yield f"http://127.0.0.1:{server.server_port}{mount}"
        finally:
            server.shutdown()
            thread.join()


def diagram_corpus(site):
    """Discover every generated HTML page that contains a Mermaid fence."""
    corpus = []
    for path in sorted(site.rglob("*.html")):
        parser = DiagramCounter()
        parser.feed(path.read_text(encoding="utf-8"))
        if parser.count:
            corpus.append((path.relative_to(site).as_posix(), parser.count))
    if not corpus:
        raise AssertionError("No published Mermaid diagrams found")
    return corpus


def check_diagrams(site, deployments, artifacts):
    """Require every published diagram to render in both themes and mounts.

    Counts come from immutable HTML, not the rendered DOM, so a skipped fence
    cannot reduce the expected coverage. A fresh context per theme prevents
    local-storage preferences from leaking between runs. Continue after a page
    failure to produce a complete corpus report, then fail the derivation.
    """
    corpus = diagram_corpus(site)
    results = []
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True, args=["--no-sandbox"])
        try:
            for name, base in deployments:
                for theme in ("dark", "light"):
                    context = browser.new_context(reduced_motion="reduce")
                    context.add_init_script(
                        f"if (location.protocol === 'http:') localStorage.setItem('cf-docs-theme', {json.dumps(theme)})")
                    context.tracing.start(screenshots=True, snapshots=True, sources=True)
                    page = context.new_page()
                    errors = []
                    page.on("pageerror", lambda error: errors.append(str(error)))
                    page.on("console", lambda message: errors.append(message.text)
                            if message.type == "error" else None)
                    page.on("requestfailed", lambda request: errors.append(f"Request failed: {request.url}"))
                    page.on("response", lambda response: errors.append(f"HTTP {response.status}: {response.url}")
                            if response.status >= 400 else None)
                    page.on("request", lambda request: errors.append(f"Request escaped mount: {request.url}")
                            if not request.url.startswith(base) else None)
                    try:
                        for relative, count in corpus:
                            errors.clear()
                            result = {"deployment": name, "theme": theme, "page": relative, "diagrams": count}
                            try:
                                response = page.goto(base + quote(relative, safe="/"))
                                assert response and response.ok, f"Page did not load: {relative}"
                                expect(page.locator("html")).to_have_attribute("data-theme", theme)
                                figures = page.get_by_role("figure", name="Diagram ")
                                expect(figures).to_have_count(count)
                                for index in range(count):
                                    figure = page.get_by_role("figure", name=f"Diagram {index + 1}", exact=True)
                                    expect(figure).to_have_attribute("data-rendered", "true", timeout=30000)
                                    expect(figure).to_have_attribute("data-render-theme", theme)
                                    expect(figure.locator("svg")).to_have_count(1)
                                    expect(figure.locator("svg")).to_be_visible()
                                    assert figure.locator(".error-icon, .error-text").count() == 0
                                assert not errors, "\n".join(errors)
                                result["passed"] = True
                            except Exception as error:
                                result.update(passed=False, error=str(error), browser_errors=list(errors))
                                page.screenshot(path=str(artifacts / f"corpus-{name}-{theme}-{len(results)}-failure.png"),
                                                full_page=True)
                                print(json.dumps(result), flush=True)
                            results.append(result)
                    finally:
                        context.tracing.stop(path=str(artifacts / f"corpus-{name}-{theme}-trace.zip"))
                        context.close()
        finally:
            browser.close()
    report = {"pages": len(corpus), "diagrams": sum(count for _, count in corpus),
              "passed": all(result["passed"] for result in results), "results": results}
    (artifacts / "mermaid-corpus.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    assert report["passed"], "Mermaid corpus failed; see screenshots/mermaid-corpus.json"
    print(f"Mermaid corpus: {report['diagrams']} diagrams on {report['pages']} pages, both themes and mounts passed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--site", type=Path, required=True)
    parser.add_argument("--browser-test", type=Path, required=True)
    parser.add_argument("--screenshots", type=Path, required=True)
    args = parser.parse_args()
    args.screenshots.mkdir(parents=True, exist_ok=True)
    with ExitStack() as stack:
        base = stack.enter_context(serve(args.site, "/"))
        prefix = stack.enter_context(serve(args.site, "/project/site/"))
        smoke = subprocess.run([sys.executable, str(args.browser_test), "--base-url", base,
                                "--prefix-url", prefix, "--screenshots", str(args.screenshots)], check=False)
        # Even a smoke-test failure must not hide unrelated corpus errors.
        check_diagrams(args.site, (("base", base), ("prefix", prefix)), args.screenshots)
        if smoke.returncode:
            raise SystemExit(smoke.returncode)


if __name__ == "__main__":
    main()
