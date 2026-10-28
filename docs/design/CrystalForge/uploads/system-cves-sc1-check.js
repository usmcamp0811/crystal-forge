// Run in a Nix shell with playwright-test and playwright-driver available.
// This design-only harness does not contact Crystal Forge APIs or alter fixtures.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { execFileSync } = require("node:child_process");

// The Nix playwright CLI wrapper carries its matching browser path. Node does
// not receive wrapper exports when launched directly; recover that one value.
if (!process.env.PLAYWRIGHT_BROWSERS_PATH) {
  const wrapper = fs.readFileSync(execFileSync("which", ["playwright"], { encoding: "utf8" }).trim(), "utf8");
  const match = wrapper.match(/export PLAYWRIGHT_BROWSERS_PATH=.*?'(\/nix\/store\/[^']+)'/);
  if (!match) throw new Error("The Nix playwright wrapper did not name its matching browser path");
  process.env.PLAYWRIGHT_BROWSERS_PATH = match[1];
}
const { chromium } = require("playwright");

const origin = process.argv[2] || "http://127.0.0.1:39997";
const output = process.argv[3] || "/tmp/opencode";
const cases = [
  ["tracked-a", "openssl"], ["local-proved", "openssl"],
  ["mapped-read-only", "read-only"], ["mapped-clean", "No findings in the selected scan"],
  ["unmapped", "Running configuration is unmapped"],
  ["ambiguous", "Running target is ambiguous"],
  ["no-report", "No running configuration reported"],
  ["invalid-report", "Running target unavailable"],
  ["known-no-scan", "No completed CVE scan for this target"],
  ["completed-empty", "No findings in the selected scan"],
  ["unknown-only", "unknown"], ["failed-rescan", "openssl"],
  ["queued-rescan", "openssl"], ["read-loading", "Loading the selected inventory"],
  ["read-error", "Unable to load CVE inventory"],
  ["retry-failed", "Unable to load CVE inventory"],
  ["menu-error", "Revision choices could not be refreshed"],
  ["continuation-error", "More findings could not be loaded"],
  ["evaluated-b", "openssl"], ["activated-b", "curl"],
];

(async () => {
  const browser = await chromium.launch();
  try {
    for (const [width, height, size] of [[1440, 900, "desktop"], [900, 768, "narrow"]]) {
      const context = await browser.newContext({ viewport: { width, height } });
      await context.addInitScript(() => localStorage.setItem("cf.coach.v1", JSON.stringify({ done: [], panel: "dismissed", calloutHidden: {} })));
      const page = await context.newPage();
      const pageErrors = [];
      page.on("pageerror", error => pageErrors.push(error.message));
      await page.goto(`${origin}/crystal-forge.html?sc1=tracked-a`);
      await page.locator('[data-screen-label="SystemDetail-orion-db-02"]').waitFor();
      await page.getByText("Vulnerabilities", { exact: true }).waitFor();

      for (const theme of ["dark", "light"]) {
        await page.evaluate(value => document.documentElement.setAttribute("data-theme", value), theme);
        for (const [key, expected] of cases) {
          await page.evaluate(value => window.dispatchEvent(new CustomEvent("cf-sc1-design-state", { detail: { key: value } })), key);
          await page.getByText(expected, { exact: false }).first().waitFor();
          if (["unmapped", "ambiguous", "no-report", "invalid-report"].includes(key)) {
            assert.equal(await page.getByText("CVE-2026-11999", { exact: true }).count(), 0, `${key} substituted head B`);
          }
          if (key === "mapped-read-only") {
            assert.equal(await page.getByRole("button", { name: /Triage —/ }).count(), 0);
            await page.getByText("Retained deployment proof is unavailable", { exact: false }).waitFor();
          }
          if (key === "known-no-scan") {
            assert.equal(await page.getByText("No findings in the selected scan").count(), 0);
          }
          if (key === "evaluated-b") {
            assert.equal(await page.getByText("CVE-2026-11999", { exact: true }).count(), 0,
              "evaluating B replaced running A's scan");
          }
          if (key === "continuation-error") {
            await page.getByText("2 of 5 shown", { exact: false }).waitFor();
            await page.getByText("CVE-2026-11801", { exact: true }).waitFor();
          }
          if (key === "unknown-only") {
            assert.equal(await page.getByText("No findings in the selected scan").count(), 0,
              "an unknown-severity finding was presented as clean");
          }
          const screen = path.join(output, `sc1-${key}-${size}-${theme}.png`);
          await page.screenshot({ path: screen, fullPage: true });
        }
      }

      await page.evaluate(() => window.dispatchEvent(new CustomEvent("cf-sc1-design-state", { detail: { key: "unmapped" } })));
      await page.getByText("Running configuration is unmapped").waitFor();
      await page.getByRole("button", { name: "Commits" }).click();
      const selector = page.getByRole("combobox", { name: "Scan target" });
      await selector.selectOption("commit:a3f8c12");
      await page.getByText("CVE-2026-11999", { exact: true }).waitFor();
      await page.reload();
      await page.locator('[data-screen-label="SystemDetail-orion-db-02"]').waitFor();
      await page.getByText("CVE-2026-11999", { exact: true }).waitFor();
      await page.evaluate(() => window.dispatchEvent(new CustomEvent("cf-sc1-design-state", { detail: { key: "unmapped" } })));
      await selector.selectOption("current");
      await page.getByText("Running configuration is unmapped").waitFor();
      await page.goBack();
      await page.getByText("CVE-2026-11999", { exact: true }).waitFor();
      await selector.selectOption("current");
      await page.getByText("Running configuration is unmapped").waitFor();

      await page.evaluate(() => window.dispatchEvent(new CustomEvent("cf-sc1-design-state", { detail: { key: "read-error" } })));
      await page.getByRole("button", { name: "Retry inventory read" }).click();
      assert.equal(await page.evaluate(() => document.activeElement?.getAttribute("aria-label")), "Scan target", "retry lost keyboard focus");
      await page.getByText("Retrying inventory read").waitFor();
      await page.getByText("CVE-2026-11801", { exact: true }).waitFor();
      await page.evaluate(() => window.dispatchEvent(new CustomEvent("cf-sc1-design-state", { detail: { key: "retry-failed" } })));
      await page.getByRole("button", { name: "Retry inventory read" }).focus();
      await page.keyboard.press("Enter");
      await page.getByText("The selected inventory read failed again.").waitFor();
      await page.evaluate(() => window.dispatchEvent(new CustomEvent("cf-sc1-design-state", { detail: { key: "menu-error" } })));
      await page.getByText("Revision choices could not be refreshed").waitFor();
      await page.getByText("CVE-2026-11801", { exact: true }).waitFor();

      await page.evaluate(() => window.dispatchEvent(new CustomEvent("cf-sc1-design-state", { detail: { key: "tracked-a" } })));
      await page.getByText("CVE-2026-11801", { exact: true }).waitFor();
      await page.getByRole("button", { name: "Generations" }).click();
      await selector.selectOption("generation:191");
      await page.getByText("CVE-2026-11001", { exact: true }).waitFor();
      await page.getByRole("button", { name: "Commits" }).click();
      assert.equal(await selector.inputValue(), "generation:191", "presentation mode changed exact target intent");
      await page.getByRole("tab", { name: /Hardening/ }).click();
      await page.getByText("Audited config").waitFor();
      await page.getByRole("tab", { name: /CVEs/ }).click();
      assert.equal(await selector.inputValue(), "generation:191", "tab switch discarded exact target");
      await page.reload();
      await page.locator('[data-screen-label="SystemDetail-orion-db-02"]').waitFor();
      assert.equal(await selector.inputValue(), "generation:191", "reload discarded exact target");
      await page.getByText("CVE-2026-11001", { exact: true }).waitFor();
      await selector.selectOption("current");
      await page.getByText("CVE-2026-11801", { exact: true }).waitFor();
      await page.getByRole("button", { name: /Triage — accept the risk/ }).first().click();
      const justification = page.getByPlaceholder("Why is this acceptable / what is the compensating control?");
      await page.getByRole("button", { name: "Accept risk" }).first().click();
      await justification.fill("Design review baseline remains from the original running scan.");
      await page.evaluate(() => window.dispatchEvent(new CustomEvent("cf-sc1-design-state", { detail: { key: "activated-b" } })));
      await page.getByText("Current evidence changed while this draft was open").waitFor();
      assert.equal(await justification.inputValue(), "Design review baseline remains from the original running scan.");
      assert.equal(await page.getByRole("button", { name: "Apply triage" }).isDisabled(), true);
      await page.getByRole("button", { name: "Cancel" }).click();
      await page.getByRole("tab", { name: /Hardening/ }).click();
      await page.getByText("Audited config").waitFor();
      await page.getByRole("tab", { name: /CVEs/ }).click();
      await page.getByRole("combobox", { name: "Scan target" }).waitFor();
      assert.deepEqual(pageErrors, [], `browser errors at ${size}`);

      const baseline = await context.newPage();
      baseline.on("pageerror", error => pageErrors.push(error.message));
      await baseline.goto(`${origin}/crystal-forge.html`);
      await baseline.locator(".app").waitFor();
      await baseline.evaluate(() => window.dispatchEvent(new CustomEvent("cf-open-system", { detail: { hostname: "orion-db-02", tab: "cves" } })));
      await baseline.getByText("Vulnerabilities", { exact: true }).waitFor();
      for (const label of ["CVE", "Severity", "CVSS", "Fix", "Triage"]) {
        assert.equal(await baseline.getByRole("columnheader", { name: label, exact: true }).count(), 1, `normal ${label} column changed`);
      }
      for (const theme of ["dark", "light"]) {
        await baseline.evaluate(value => document.documentElement.setAttribute("data-theme", value), theme);
        await baseline.screenshot({ path: path.join(output, `sc1-normal-${size}-${theme}.png`), fullPage: true });
      }
      await baseline.getByRole("tab", { name: /Hardening/ }).click();
      await baseline.getByText("Audited config").waitFor();
      await baseline.locator(".sidebar .nav-item").filter({ hasText: "CVEs" }).click();
      await baseline.getByRole("heading", { name: "CVEs", exact: true }).waitFor();
      await baseline.getByRole("button", { name: "Flat list" }).click();
      await baseline.getByTitle("Details").first().click();
      await baseline.getByText("Triage status").waitFor();
      await baseline.getByText("Triage status").locator("..").getByRole("button").click();
      await baseline.getByRole("heading", { name: /Triage CVE-/ }).waitFor();
      assert.equal(await baseline.getByText("Current evidence changed while this draft was open").count(), 0,
        "System Detail's optional draft conflict leaked into fleet triage");
      assert.deepEqual(pageErrors, [], `browser errors at ${size}`);
      await baseline.close();
      await context.close();
    }
    console.log(`SC1 design states: ${cases.length} cases × 2 widths × 2 themes; explicit head and retry assertions passed.`);
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
