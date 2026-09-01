/**
 * Renders observable states from the authoritative React design.
 *
 * Usage:
 *   node generate-design-targets.js <designDir> <manifest> <outputDir>
 *   node generate-design-targets.js --validate-manifest <manifest>
 *
 *   <designDir>   Directory containing the offline design example
 *                 (crystal-forge.html + vendored react/babel + assets).
 *   <manifest>    Path to design-parity/manifest.json.
 *   <outputDir>   Where <view>--<theme>.design.png files are written.
 *
 * The design example must be reachable via file:// with all scripts vendored
 * locally (no network). The manifest drives the design's real navigation and
 * identifies the expected rendered surface before a screenshot is accepted.
 */
const fs = require("fs");
const path = require("path");

function task440Targets(manifest) {
  return manifest.targets?.task440 || [];
}

function normalizedTexts(values) {
  return values.map((value) => value.replace(/\s+/g, " ").trim());
}

function expectedRows(contract, side, theme) {
  const rows = contract.orderedVisibleRows;
  if (Array.isArray(rows)) return rows;
  const sideRows = rows?.[side];
  return Array.isArray(sideRows) ? sideRows : sideRows?.[theme];
}

async function exactTexts(locator) {
  return normalizedTexts(await locator.allTextContents());
}

function assertEqual(actual, expected, label) {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(`${label}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
  }
}

async function validateSemanticContract(page, capture, contract, theme) {
  if (!contract) throw new Error(`${capture.name}: semantic fixture contract is missing`);
  if (contract.kind !== capture.designState.kind) throw new Error(`${capture.name}: semantic contract kind does not match the manifest`);

  if (contract.kind === "system-config") {
    await page.locator(".cfg-count:not(:text-is('Querying…'))").waitFor({ state: "visible" });
    assertEqual(await exactTexts(page.locator(".cfg-table tbody > tr.cfg-row .cfg-path")), expectedRows(contract, "react", theme), `${capture.name} ordered Config rows`);
    assertEqual(await exactTexts(page.locator(".cfg-toolbar .seg button")), [
      `All ${contract.counts.all.toLocaleString("en-US")}`,
      `Overridden ${contract.counts.overridden}`,
      `Changed ${contract.counts.changed}`,
    ], `${capture.name} Config counts`);
    if (contract.searchQuery) assertEqual(await page.getByPlaceholder("Filter options, values, modules…").inputValue(), contract.searchQuery, `${capture.name} Config search`);
    const selected = page.locator("select.cfg-revselect option:checked");
    assertEqual(await selected.getAttribute("value"), String(contract.identity.generation), `${capture.name} selected generation`);
    const selectedText = normalizedTexts([await selected.textContent()]);
    if (!selectedText[0].includes(contract.identity.revision)) throw new Error(`${capture.name}: selected generation lost revision ${contract.identity.revision}`);
    assertEqual(normalizedTexts([await page.locator(".cfg-revbar-msg").textContent()])[0], contract.identity.revisionMessage, `${capture.name} revision message`);
    await page.getByText(contract.identity.uptime.react, { exact: true }).waitFor({ state: "visible" });
    await page.locator(".sd-metric-sub").filter({ hasText: `activated · ${contract.identity.heartbeatAgeMinutes}m ago` }).waitFor({ state: "visible" });
    const evaluation = page.locator(".cfg-side > section").nth(1);
    for (const expected of [
      contract.counts.all.toLocaleString("en-US"),
      `${contract.counts.hostDelta} rows`,
      String(contract.counts.packages),
      contract.formattedMetrics.reactClosure,
    ]) await evaluation.getByText(expected, { exact: true }).waitFor({ state: "visible" });
    const evalTime = normalizedTexts([await evaluation.locator(".sd-drift-row").filter({ hasText: "Eval time" }).textContent()])[0];
    if (!evalTime.includes(contract.formattedMetrics.reactEvaluationDuration)) throw new Error(`${capture.name}: evaluation duration mismatch: ${evalTime}`);
    if (contract.expandedItem) {
      const row = page.getByText(contract.expandedItem, { exact: true }).locator("xpath=ancestor::tr[1]");
      if ((await row.getAttribute("class") || "").split(/\s+/).includes("open") === false) throw new Error(`${capture.name}: ${contract.expandedItem} is not expanded`);
    }
  } else {
    const tray = page.getByRole("dialog", { name: `${contract.identity.flake} commits` });
    await tray.getByText(contract.identity.revision, { exact: true }).first().waitFor({ state: "visible" });
    await tray.locator(".fx-revbar-msg").filter({ hasText: contract.identity.revisionMessage }).waitFor({ state: "visible" });
    await tray.getByRole("button", { name: new RegExp(`^${contract.selectedPane}\\b`) }).waitFor({ state: "visible" });
    let rowLocator;
    if (contract.selectedPane === "Systems") rowLocator = tray.locator(".fx-pane .fx-table tbody > tr .fx-host");
    else if (contract.selectedPane === "Modules") rowLocator = tray.locator(".fx-pane .fx-table > tbody > tr.fx-row .fx-host");
    else rowLocator = tray.locator(".fx-input-cell .fx-host");
    assertEqual(await exactTexts(rowLocator), expectedRows(contract, "react", theme), `${capture.name} ordered ${contract.selectedPane} rows`);
    if (contract.orderedExpandedRows) assertEqual(await exactTexts(tray.locator(".fx-opts .fx-opt-path")), contract.orderedExpandedRows, `${capture.name} expanded declarations`);
    if (contract.expandedItem) {
      const row = tray.getByText(contract.expandedItem, { exact: true }).locator("xpath=ancestor::tr[1]");
      if (!(await row.getAttribute("class") || "").split(/\s+/).includes("fx-row")) throw new Error(`${capture.name}: expanded module row is missing`);
      await tray.getByText("Declared options", { exact: true }).waitFor({ state: "visible" });
    }
  }
}

function validateManifest(manifest) {
  const errors = [];
  const names = new Set();
  const steps = new Set();
  if (!Array.isArray(manifest.views) || manifest.views.length === 0) {
    errors.push("views must be a non-empty array");
  }
  for (const [index, view] of (manifest.views || []).entries()) {
    const label = view?.name || `views[${index}]`;
    if (!view?.name || names.has(view.name)) errors.push(`duplicate or missing capture name: ${label}`);
    names.add(view?.name);
    if (typeof view?.route !== "string") errors.push(`${label}.route must be a string`);
    if (!view?.designMarker || typeof view.designMarker.selector !== "string") errors.push(`${label}.designMarker.selector must be a string`);
    if (!view?.dioxusMarker || typeof view.dioxusMarker.selector !== "string") errors.push(`${label}.dioxusMarker.selector must be a string`);
    for (const actionField of ["designActions", "dioxusActions"]) {
      for (const [actionIndex, action] of (view?.[actionField] || []).entries()) {
        if (action?.type !== "click" || typeof action.selector !== "string") errors.push(`${label}.${actionField}[${actionIndex}] must be a click action with a selector`);
        if (action?.force !== undefined && typeof action.force !== "boolean") errors.push(`${label}.${actionField}[${actionIndex}].force must be a boolean`);
      }
    }
  }
  for (const target of task440Targets(manifest)) {
    if (!target.name || names.has(target.name)) errors.push(`duplicate or missing capture name: ${target.name || "<missing>"}`);
    names.add(target.name);
    if (!["system-config", "flake-pane"].includes(target.designState?.kind)) errors.push(`${target.name}: unsupported or missing designState.kind`);
    if (!Number.isInteger(target.viewport?.width) || !Number.isInteger(target.viewport?.height)) errors.push(`${target.name}: missing integer viewport`);
    if (!target.dioxusStep || steps.has(target.dioxusStep)) errors.push(`${target.name}: duplicate or missing dioxusStep`);
    steps.add(target.dioxusStep);
    if (!Array.isArray(target.designState.expectedText) || target.designState.expectedText.length === 0) errors.push(`${target.name}: expectedText must identify the rendered target`);
    if (!target.contentSelector) errors.push(`${target.name}: contentSelector is required`);
  }
  if (!Array.isArray(manifest.settings?.themes) || manifest.settings.themes.length === 0) errors.push("settings.themes must not be empty");
  if (errors.length) throw new Error(`Invalid design-parity manifest: ${errors.join("; ")}`);
}

async function applyTheme(page, theme) {
  const root = page.locator("html");
  if ((await root.getAttribute("data-theme")) !== theme) {
    const toggle = page.getByRole("button", { name: "Toggle theme" });
    await toggle.waitFor({ state: "visible" });
    // Nested trays cover the topbar. A DOM click still invokes the real React
    // control instead of mutating the theme attribute behind the application.
    await toggle.evaluate((button) => button.click());
  }
  await page.waitForFunction(
    (expected) => document.documentElement.getAttribute("data-theme") === expected,
    theme,
  );
}

async function expectScreen(page, screen, heading) {
  await page.locator(`[data-screen-label="${screen}"]`).waitFor({ state: "visible" });
  await page.getByRole("heading", { name: heading, exact: true }).waitFor({ state: "visible" });
}

async function drivePrimary(page, state) {
  if (state.navigation) {
    await page.locator("aside").locator("span").getByText(state.navigation, { exact: true }).click();
  }
  await expectScreen(page, state.screen, state.heading);
}

async function driveSystemConfig(page, state) {
  await page.evaluate(({ hostname, tab }) => {
    window.dispatchEvent(new CustomEvent("cf-open-system", { detail: { hostname, tab } }));
  }, state);
  await page.locator('[data-screen-label="SystemDetail"]').waitFor({ state: "visible" });
  await page.getByRole("tab", { name: "Config", selected: true }).waitFor({ state: "visible" });
  await page.getByRole("heading", { name: "Evaluated options" }).waitFor({ state: "visible" });
  if (state.search) {
    await page.getByPlaceholder("Filter options, values, modules…").fill(state.search);
    await page.locator(".cfg-count").getByText("1–1 of 1", { exact: true }).waitFor({ state: "visible" });
  }
  if (state.expandOption) {
    const option = page.getByText(state.expandOption, { exact: true });
    await option.waitFor({ state: "visible" });
    const row = page.getByRole("row").filter({ has: page.getByText(state.expandOption, { exact: true }) });
    await row.click();
    await row.locator("xpath=following-sibling::tr[1]").getByText("Definitions", { exact: true }).waitFor({ state: "visible" });
  }
}

async function driveFlakePane(page, state) {
  await page.locator("aside").locator("span").getByText("Flakes", { exact: true }).click();
  await expectScreen(page, "flakes", "Flakes");
  await page.getByRole("button", { name: "Table" }).click();
  const flakeRow = page.getByRole("row").filter({ has: page.getByText(state.flake, { exact: true }) });
  await flakeRow.click();
  const tray = page.getByRole("dialog", { name: `${state.flake} commits` });
  await tray.waitFor({ state: "visible" });
  const paneButton = tray.getByRole("button", { name: new RegExp(`^${state.pane}\\b`) });
  await paneButton.click();
  await page.waitForFunction(
    ({ flake, pane }) => {
      const tray = document.querySelector(`aside[aria-label="${flake} commits"]`);
      return [...(tray?.querySelectorAll(".fx-tab") || [])].some((button) => button.classList.contains("active") && button.textContent.trim().startsWith(pane));
    },
    { flake: state.flake, pane: state.pane },
  );
  if (state.expandModule) {
    const moduleRow = tray.getByRole("row").filter({ has: page.getByText(state.expandModule, { exact: true }) });
    await moduleRow.click();
    await tray.getByText("Declared options", { exact: true }).waitFor({ state: "visible" });
  }
}

async function driveState(page, state) {
  if (state.kind === "primary") await drivePrimary(page, state);
  else if (state.kind === "system-config") await driveSystemConfig(page, state);
  else if (state.kind === "flake-pane") await driveFlakePane(page, state);
  else throw new Error(`Unsupported design state: ${state.kind}`);

  for (const text of state.expectedText || []) {
    await page.getByText(text, { exact: true }).first().waitFor({ state: "visible" });
  }
}

async function validateState(page, state) {
  if (state.kind === "primary") {
    await expectScreen(page, state.screen, state.heading);
  } else if (state.kind === "system-config") {
    await page.getByRole("tab", { name: "Config", selected: true }).waitFor({ state: "visible" });
    await page.getByRole("heading", { name: "Evaluated options" }).waitFor({ state: "visible" });
  } else if (state.kind === "flake-pane") {
    await page.waitForFunction(
      ({ flake, pane }) => {
        const tray = document.querySelector(`aside[aria-label="${flake} commits"]`);
        return [...(tray?.querySelectorAll(".fx-tab") || [])].some((button) => button.classList.contains("active") && button.textContent.trim().startsWith(pane));
      },
      { flake: state.flake, pane: state.pane },
    );
  }
  for (const text of state.expectedText || []) {
    await page.getByText(text, { exact: true }).first().waitFor({ state: "visible" });
  }
}

function actionLocator(page, action) {
  let locator = page.locator(action.selector);
  if (action.text) locator = locator.filter({ hasText: action.text });
  return locator.nth(action.index || 0);
}

async function runActions(page, actions = []) {
  for (const action of actions) {
    const locator = actionLocator(page, action);
    await locator.waitFor({ state: "visible", timeout: action.timeout || 15000 });
    await locator.click({ force: action.force === true });
    if (action.waitFor) {
      await page.locator(action.waitFor).first().waitFor({ state: "visible", timeout: action.timeout || 15000 });
    }
  }
}

async function assertMarker(page, marker, label) {
  const locator = page.locator(marker.selector).first();
  await locator.waitFor({ state: "visible", timeout: marker.timeout || 15000 });
  if (marker.text) {
    const text = (await locator.textContent()) || "";
    if (!text.includes(marker.text)) throw new Error(`${label} marker ${marker.selector} did not contain ${JSON.stringify(marker.text)}`);
  }
  if (marker.attribute) {
    const value = await locator.getAttribute(marker.attribute);
    if (value !== marker.value) throw new Error(`${label} marker ${marker.selector} had ${marker.attribute}=${JSON.stringify(value)}, expected ${JSON.stringify(marker.value)}`);
  }
}

async function main() {
  if (process.argv[2] === "--validate-manifest") {
    const manifest = JSON.parse(fs.readFileSync(process.argv[3], "utf8"));
    validateManifest(manifest);
    console.log(`Manifest valid: ${manifest.views.length} primary views, ${task440Targets(manifest).length} TASK-440 targets`);
    return;
  }

  const designDir = process.argv[2];
  const manifestPath = process.argv[3];
  const outputDir = process.argv[4] || "/tmp/design-targets";
  if (!designDir || !manifestPath) throw new Error("usage: node generate-design-targets.js <designDir> <manifest> <outputDir>");
  const { chromium } = require("playwright");

  const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  validateManifest(manifest);
  const fixturePath = path.resolve(path.dirname(manifestPath), manifest.settings.semanticFixture);
  const fixture = JSON.parse(fs.readFileSync(fixturePath, "utf8"));
  const semanticTargets = fixture.task440?.semanticTargets || {};
  const designFixture = JSON.parse(fs.readFileSync(path.join(designDir, "fixtures", "crystal-forge.fixtures.json"), "utf8"));
  const themes = manifest.settings.themes;
  const selectedTask440 = new Set((process.env.CF_TASK440_TARGETS || "").split(",").filter(Boolean));
  const focused = selectedTask440.size > 0;
  const captures = [
    ...(focused ? [] : manifest.views.map((view) => ({ ...view, group: "primary", viewport: manifest.settings.viewport }))),
    ...task440Targets(manifest).filter((target) => !focused || selectedTask440.has(target.name)).map((target) => ({ ...target, group: "task440" })),
  ];
  const htmlPath = path.join(designDir, "crystal-forge.html");
  if (!fs.existsSync(htmlPath)) throw new Error(`design example not found at ${htmlPath}`);
  fs.mkdirSync(outputDir, { recursive: true });

  const browser = await chromium.launch({
    args: ["--allow-file-access-from-files", "--no-sandbox", "--disable-dev-shm-usage", "--disable-setuid-sandbox"],
  });
  const results = [];
  try {
    for (const capture of captures) {
      for (const theme of themes) {
        const name = `${capture.name}--${theme}`;
        const context = await browser.newContext({ viewport: capture.viewport, timezoneId: "UTC", locale: "en-US" });
        await context.addInitScript(({ systems, showCoach }) => {
          window.__fx = (key) => key === "systems" ? systems : undefined;
          if (showCoach) {
            localStorage.removeItem("cf.coach.v1");
          } else {
            localStorage.setItem("cf.coach.v1", JSON.stringify({ done: [], panel: "dismissed", calloutHidden: {} }));
          }
        }, { systems: designFixture.systems, showCoach: capture.name === "setup-coach" });
        const page = await context.newPage();
        const runtimeErrors = [];
        page.on("pageerror", (error) => runtimeErrors.push(error.message));
        page.on("requestfailed", (request) => runtimeErrors.push(`request failed: ${request.url()} (${request.failure()?.errorText || "unknown"})`));
        let error = null;
        try {
          await page.goto(`file://${htmlPath}`, { waitUntil: "load", timeout: 90_000 });
          await page.locator(".app .content").waitFor({ state: "visible", timeout: 90_000 });
          if (capture.group === "primary") {
            await runActions(page, capture.designActions);
          } else {
            await driveState(page, capture.designState);
          }
          await applyTheme(page, theme);
          if (capture.group === "primary") {
            await assertMarker(page, capture.designMarker, `${name} design target`);
          } else {
            await validateState(page, capture.designState);
          }
          const semanticContract = capture.group === "task440"
            ? { name: capture.name, ok: true }
            : null;
          if (semanticContract) await validateSemanticContract(page, capture, semanticTargets[capture.name], theme);
          if (runtimeErrors.length) throw new Error(runtimeErrors.join("; "));
          const contentSurface = capture.contentSelector ? await page.locator(capture.contentSelector).boundingBox() : null;
          if (capture.contentSelector && (!contentSurface || contentSurface.width <= 0 || contentSurface.height <= 0)) throw new Error(`${capture.name}: content surface is not measurable`);
          await page.screenshot({ path: path.join(outputDir, `${name}.design.png`), animations: "disabled" });
          console.log(`  OK design target: ${name}`);
          results.push({ name, group: capture.group, target: capture.name, theme, viewport: capture.viewport, ok: true, error: null, semanticContract, contentSurface });
        } catch (err) {
          error = err.message;
          console.error(`  FAIL design target: ${name} - ${error}`);
          await page.screenshot({ path: path.join(outputDir, `${name}.failure.png`), animations: "disabled" }).catch(() => {});
          results.push({ name, group: capture.group, target: capture.name, theme, viewport: capture.viewport, ok: false, error, semanticContract: capture.group === "task440" ? { name: capture.name, ok: false, error } : null, contentSurface: null });
        }
        await context.close();
      }
    }
  } finally {
    await browser.close();
  }

  fs.writeFileSync(path.join(outputDir, "design-targets.json"), JSON.stringify({ results }, null, 2));
  const okCount = results.filter((result) => result.ok).length;
  const task440Ok = results.filter((result) => result.group === "task440" && result.ok).length;
  const expectedTask440 = captures.filter((capture) => capture.group === "task440").length * themes.length;
  console.log(`Design targets: ${okCount}/${results.length} rendered; TASK-440: ${task440Ok}/${expectedTask440}`);
  if (okCount !== results.length) process.exitCode = 1;
}

if (require.main === module) {
  main().catch((err) => {
    console.error(`Fatal error: ${err.message}`);
    console.error(err.stack);
    process.exit(1);
  });
}

module.exports = { assertMarker, runActions, validateManifest };
