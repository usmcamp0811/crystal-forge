"""Exercise an owner-served docs site at normal and prefixed URLs with Chromium.

Does not start or stop servers. Each deployment uses a fresh browser context.
Screenshots and traces are verification artifacts, not visual golden baselines.
"""
import argparse
import json
import re
from pathlib import Path
from urllib.parse import urljoin, urlsplit

from playwright.sync_api import expect, sync_playwright


def normalize_url(value):
    parts = urlsplit(value)
    if parts.scheme not in {"http", "https"} or not parts.netloc or parts.query or parts.fragment:
        raise argparse.ArgumentTypeError("use an HTTP(S) site directory URL without query or fragment")
    return value.rstrip("/") + "/"


def exercise(browser, base, name, artifacts, diagram_page):
    context = browser.new_context(viewport={"width": 1440, "height": 1000}, reduced_motion="reduce")
    context.tracing.start(screenshots=True, snapshots=True, sources=True)
    page = context.new_page()
    errors = []
    origin = urlsplit(base)

    def local(url):
        parsed = urlsplit(url)
        return (parsed.scheme, parsed.netloc) == (origin.scheme, origin.netloc)

    def request(request):
        if local(request.url) and not urlsplit(request.url).path.startswith(origin.path):
            errors.append(f"request escaped deployment prefix: {request.url}")

    page.on("request", request)
    page.on("pageerror", lambda error: errors.append(str(error)))
    page.on("console", lambda message: errors.append(message.text) if message.type == "error" else None)
    page.on("response", lambda response: errors.append(f"HTTP {response.status}: {response.url}")
            if local(response.url) and response.status >= 400 else None)
    page.on("requestfailed", lambda request: errors.append(f"request failed: {request.url}: {request.failure}"))
    root = page.locator("html")
    main = page.get_by_role("main")
    toggle = page.get_by_role("button", name="Light theme", exact=True)
    navigation = page.get_by_role("navigation", name="Documents", exact=True)
    bundle_navigation = page.get_by_role("navigation", name="Bundles", exact=True)

    def theme(value):
        expect(toggle).to_be_visible()
        if root.get_attribute("data-theme") != value:
            toggle.click()
        expect(root).to_have_attribute("data-theme", value)
        expect(toggle).to_have_attribute("aria-pressed", str(value == "light").lower())
        background = page.evaluate("getComputedStyle(document.body).backgroundColor")
        assert background == {"dark": "rgb(3, 7, 18)", "light": "rgb(238, 242, 247)"}[value], background

    def no_overflow():
        page.wait_for_function("document.documentElement.scrollWidth <= innerWidth + 1")

    def inside_prefix():
        assert page.url.startswith(base), f"navigation escaped prefix: {page.url}"

    def diagram_ready():
        figure = page.get_by_role("figure", name="Diagram 1", exact=True)
        expect(figure).to_have_attribute("data-rendered", "true", timeout=30000)
        expect(figure).to_have_attribute("data-render-theme", root.get_attribute("data-theme"), timeout=30000)
        svg = figure.locator("svg")
        expect(svg).to_have_count(1)
        expect(svg).to_be_visible()
        assert svg.locator("path").count() > 0, "diagram has no rendered geometry"
        assert "Syntax error" not in figure.inner_text()

    try:
        page.goto(base)
        expect(main.get_by_role("heading", level=1)).to_have_count(1)
        expect(root).to_have_attribute("data-theme", "dark")
        theme("dark")
        logo = page.get_by_role("link", name="Crystal Forge documentation home").locator("img")
        expect(logo).to_be_visible()
        expect(logo).to_have_js_property("complete", True)
        assert logo.evaluate("img => img.complete && img.naturalWidth > 0"), "branding image did not load"
        no_overflow()
        page.screenshot(path=str(artifacts / f"{name}-desktop-dark.png"), full_page=True)
        theme("light")
        # Exercise real navigation after changing the preference, not only reload.
        bundle_navigation.get_by_role("link", name="Knowledge", exact=True).click()
        inside_prefix()
        expect(root).to_have_attribute("data-theme", "light")
        page.reload()
        expect(root).to_have_attribute("data-theme", "light")
        page.screenshot(path=str(artifacts / f"{name}-desktop-light.png"), full_page=True)

        search = page.get_by_role("search")
        search.get_by_role("searchbox", name="Search", exact=True).fill("status:proposed")
        search.get_by_role("button", name="Search", exact=True).click()
        inside_prefix()
        expect(main.get_by_role("status")).to_have_text(re.compile(r"[1-9]\d* results"))
        # Select the first matching result within the search results, not a
        # coincidentally matching sidebar or breadcrumb link.
        hits = main.get_by_role("list").get_by_role("link")
        expect(hits).not_to_have_count(0)
        hits.first.click()
        inside_prefix()
        expect(main.locator('[data-status="proposed"]')).to_have_text("proposed")
        expect(navigation.locator('a[aria-current="page"]')).to_have_count(1)
        expect(root).to_have_attribute("data-theme", "light")

        bundle_navigation.get_by_role("link", name="NixOS options", exact=True).click()
        inside_prefix()
        expect(main.get_by_role("heading", name="NixOS module options", exact=True)).to_be_visible()
        expect(main).to_contain_text("Defaults and examples are omitted")
        service = main.get_by_role("link", name="services.crystal-forge.server", exact=True)
        expect(service).to_be_visible()
        service.click()
        expect(main.get_by_role("heading", name="services.crystal-forge.server", exact=True)).to_be_visible()
        expect(main).to_contain_text("services.crystal-forge.server.enable")
        inside_prefix()

        page.goto(urljoin(base, diagram_page))
        diagram_ready()
        for value in ("light", "dark"):
            theme(value)
            diagram_ready()
            no_overflow()
            page.screenshot(path=str(artifacts / f"{name}-diagram-{value}.png"), full_page=True)

        page.set_viewport_size({"width": 390, "height": 844})
        page.goto(base)
        mobile_toggle = page.get_by_role("button", name="Browse docs", exact=True)
        for value in ("dark", "light"):
            theme(value)
            expect(mobile_toggle).to_be_visible()
            expect(navigation).not_to_be_visible()
            no_overflow()
            page.screenshot(path=str(artifacts / f"{name}-mobile-{value}.png"), full_page=True)
            mobile_toggle.click()
            expect(mobile_toggle).to_have_attribute("aria-expanded", "true")
            expect(navigation).to_be_visible()
            no_overflow()
            page.keyboard.press("Escape")
            expect(mobile_toggle).to_be_focused()
            expect(navigation).not_to_be_visible()
            mobile_toggle.press("Enter")
            navigation.locator('a[aria-current="page"]').click()
            inside_prefix()
            expect(navigation).not_to_be_visible()
            expect(root).to_have_attribute("data-theme", value)

        assert not errors, "\n".join(errors)
        return {"deployment": name, "base_url": base, "passed": True, "browser_errors": errors}
    except Exception:
        page.screenshot(path=str(artifacts / f"{name}-failure.png"), full_page=True)
        raise
    finally:
        context.tracing.stop(path=str(artifacts / f"{name}-trace.zip"))
        context.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", required=True, type=normalize_url)
    parser.add_argument("--prefix-url", required=True, type=normalize_url)
    parser.add_argument("--screenshots", required=True, type=Path)
    parser.add_argument("--chromium", type=Path)
    parser.add_argument("--diagram-page", default="knowledge/architecture/ecosystem-architecture-summary.html")
    args = parser.parse_args()
    if urlsplit(args.prefix_url).path == "/" or args.prefix_url == args.base_url:
        parser.error("prefix URL must use a non-root path and differ from the base URL")
    args.screenshots.mkdir(parents=True, exist_ok=True)
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(
            executable_path=str(args.chromium) if args.chromium else None,
            headless=True, args=["--no-sandbox"])
        reports = []
        try:
            for name, url in (("base", args.base_url), ("prefix", args.prefix_url)):
                reports.append(exercise(browser, url, name, args.screenshots, args.diagram_page))
        finally:
            browser.close()
        report = {"passed": True, "deployments": reports}
        (args.screenshots / "report.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps(report))


if __name__ == "__main__":
    main()
