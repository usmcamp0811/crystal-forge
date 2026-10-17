// Browser helper for a native NixOS integration check's browser spec.
//
// Usage (inside a scenario VM; normally called by run_browser in driver.py):
//   cf-browser --spec FILE.cjs --case NAME --base-url URL --out DIR
//              --facts FILE.json --receipts FILE.json [--viewport desktop]
//
// A spec is a CommonJS module that exports `async run(ctx)`. The context API
// is documented on `makeContext` below.
//
// Output contract (DIR):
//   result.json   {status: "passed"|"failed", step, error, receipts, visuals}
//   console.log   browser console messages, page errors, and failed requests
//   trace.zip     Playwright trace; kept only when the spec fails
//   *.png         named screenshots and visual comparison images
//   downloads/    files saved by ctx.download
// Exit status: 0 passed, 1 failed, 2 usage error.
//
// INVARIANT: this runner never updates a baseline. Visual comparison renders
// the approved design reference in the same run and compares it with the
// production page; there is no stored image that a run can overwrite.

"use strict";

const fs = require("fs");
const path = require("path");
const { execFileSync } = require("child_process");
const { chromium } = require("playwright");
const { expect } = require("@playwright/test");

function parseArgs(argv) {
  const args = { viewport: "desktop" };
  for (let i = 0; i < argv.length; i += 2) {
    const key = argv[i];
    const value = argv[i + 1];
    if (!key.startsWith("--") || value === undefined) {
      throw new Error(`invalid argument near ${key}`);
    }
    args[key.slice(2).replace(/-([a-z])/g, (_, c) => c.toUpperCase())] = value;
  }
  for (const required of ["spec", "case", "baseUrl", "out", "facts", "receipts"]) {
    if (!args[required]) throw new Error(`missing --${required}`);
  }
  return args;
}

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

// Builds the spec context.
//
// ctx.page, ctx.context, ctx.browser  Playwright objects for the production UI.
// ctx.expect                          Playwright expect, bounded by the profile
//                                     timeout.
// ctx.facts                           authored facts (independent expectations).
// ctx.receipts                        receipts recorded before this spec ran.
// ctx.emit(key, value)                records a new runtime receipt.
// ctx.url(path)                       absolute production URL for `path`.
// ctx.step(name, fn)                  runs a named step; failures name it.
// ctx.screenshot(name)                saves DIR/<name>.png of the page.
// ctx.download(name, trigger)         runs `trigger()`, saves the download to
//                                     DIR/downloads/<name>-<suggested name>,
//                                     and returns {path, suggestedFilename}.
// ctx.whoami()                        GET /api/auth/whoami from the page's
//                                     cookie context; returns parsed JSON.
// ctx.visual.compare(name, options)   renders the approved design reference
//                                     view and compares it with the current
//                                     page. See compareVisual.
function makeContext({ args, profile, browser, context, page, facts, receiptsIn, out, state }) {
  const timeout = profile.defaultTimeoutMs;
  const boundedExpect = expect.configure({ timeout });
  return {
    browser,
    context,
    page,
    profile,
    facts,
    receipts: receiptsIn,
    expect: boundedExpect,
    url: (p) => new URL(p, args.baseUrl).toString(),
    emit(key, value) {
      if (key in state.receipts && state.receipts[key] !== value) {
        throw new Error(`receipt ${key} emitted twice with different values`);
      }
      state.receipts[key] = value;
      return value;
    },
    async step(name, fn) {
      state.step = name;
      console.log(`[cf-browser ${args.case}] step: ${name}`);
      const result = await fn();
      return result;
    },
    async screenshot(name) {
      const file = path.join(out, `${name}.png`);
      await page.screenshot({ path: file, fullPage: false, animations: "disabled" });
      return file;
    },
    async download(name, trigger) {
      fs.mkdirSync(path.join(out, "downloads"), { recursive: true });
      const [download] = await Promise.all([page.waitForEvent("download", { timeout }), trigger()]);
      const suggestedFilename = download.suggestedFilename();
      const file = path.join(out, "downloads", `${name}-${suggestedFilename}`);
      await download.saveAs(file);
      return { path: file, suggestedFilename };
    },
    async whoami() {
      return page.evaluate(async () => {
        const response = await fetch("/api/auth/whoami", { credentials: "same-origin" });
        return { status: response.status, body: await response.json() };
      });
    },
    visual: {
      compare: (name, options) => compareVisual({ browser, page, profile, out, state, name, options }),
    },
  };
}

// Compares the current production page with the approved design reference.
//
// options:
//   view      design `?view=` value (for example "dashboard").
//   theme     "dark" | "light" (default "dark").
//   referenceSelector  element in the design page to capture (default ".app").
//   actualSelector     element in the production page to capture.
//   maxDifferentRatio  REQUIRED explicit threshold (0..1) of differing pixels.
//                      Each caller states its justification in the spec.
//
// Writes <name>.reference.png, <name>.actual.png, and <name>.diff.png and
// throws when the differing-pixel ratio exceeds maxDifferentRatio.
async function compareVisual({ browser, page, profile, out, state, name, options }) {
  if (typeof options?.maxDifferentRatio !== "number") {
    throw new Error(`visual ${name}: maxDifferentRatio must be stated explicitly`);
  }
  const viewport = page.viewportSize();
  const designContext = await browser.newContext({
    viewport,
    locale: profile.locale,
    timezoneId: profile.timezoneId,
    colorScheme: profile.colorScheme,
    reducedMotion: profile.reducedMotion,
    deviceScaleFactor: profile.deviceScaleFactor,
  });
  const designPage = await designContext.newPage();
  const site = process.env.CF_IT_DESIGN_SITE;
  const theme = options.theme || "dark";
  const reference = path.join(out, `${name}.reference.png`);
  const actual = path.join(out, `${name}.actual.png`);
  const diff = path.join(out, `${name}.diff.png`);
  try {
    await designPage.goto(`file://${site}/crystal-forge.html?view=${encodeURIComponent(options.view)}&theme=${theme}`, {
      waitUntil: "load",
      timeout: 90000,
    });
    const refLocator = designPage.locator(options.referenceSelector || ".app").first();
    await refLocator.waitFor({ state: "visible", timeout: 90000 });
    await refLocator.screenshot({ path: reference, animations: "disabled" });
  } finally {
    await designContext.close();
  }
  const actualLocator = options.actualSelector ? page.locator(options.actualSelector).first() : page;
  await actualLocator.screenshot({ path: actual, animations: "disabled" });
  let differing = 0;
  try {
    execFileSync("compare", ["-metric", "AE", "-fuzz", "2%", reference, actual, diff], { stdio: "pipe" });
  } catch (error) {
    // ImageMagick `compare` exits 1 when images differ and writes the metric
    // to stderr; exit status 2 is a real error (for example, size mismatch).
    if (error.status !== 1) {
      throw new Error(`visual ${name}: compare failed: ${String(error.stderr || error.message)}`);
    }
    differing = Number.parseFloat(String(error.stderr).trim());
  }
  const size = execFileSync("identify", ["-format", "%w %h", actual]).toString().split(" ").map(Number);
  const ratio = differing / (size[0] * size[1]);
  const record = { name, view: options.view, theme, ratio, maxDifferentRatio: options.maxDifferentRatio, reference, actual, diff };
  state.visuals.push(record);
  if (!(ratio <= options.maxDifferentRatio)) {
    throw new Error(`visual ${name}: ${(ratio * 100).toFixed(2)}% pixels differ (limit ${(options.maxDifferentRatio * 100).toFixed(2)}%)`);
  }
  return record;
}

async function main() {
  let args;
  try {
    args = parseArgs(process.argv.slice(2));
  } catch (error) {
    console.error(`cf-browser: ${error.message}`);
    return 2;
  }
  const profile = readJson(process.env.CF_IT_RENDER_PROFILE);
  const viewport = profile.viewports[args.viewport];
  if (!viewport) {
    console.error(`cf-browser: unknown viewport ${args.viewport}`);
    return 2;
  }
  const out = args.out;
  fs.mkdirSync(out, { recursive: true });
  const consoleLog = fs.createWriteStream(path.join(out, "console.log"));
  const state = { step: null, receipts: {}, visuals: [] };
  const result = { case: args.case, profile: profile.name, viewport: args.viewport, status: "failed" };

  const browser = await chromium.launch({
    args: ["--allow-file-access-from-files", "--no-sandbox", "--disable-dev-shm-usage"],
  });
  const context = await browser.newContext({
    viewport,
    locale: profile.locale,
    timezoneId: profile.timezoneId,
    colorScheme: profile.colorScheme,
    reducedMotion: profile.reducedMotion,
    deviceScaleFactor: profile.deviceScaleFactor,
    acceptDownloads: true,
  });
  context.setDefaultTimeout(profile.defaultTimeoutMs);
  context.setDefaultNavigationTimeout(profile.defaultTimeoutMs * 2);
  await context.tracing.start({ screenshots: true, snapshots: true, sources: false });
  const page = await context.newPage();
  page.on("console", (msg) => consoleLog.write(`[console.${msg.type()}] ${msg.text()}\n`));
  page.on("pageerror", (error) => consoleLog.write(`[pageerror] ${error.message}\n`));
  page.on("requestfailed", (request) =>
    consoleLog.write(`[requestfailed] ${request.method()} ${request.url()} ${request.failure()?.errorText || ""}\n`),
  );
  page.on("response", (response) => {
    if (response.status() >= 400) consoleLog.write(`[http ${response.status()}] ${response.request().method()} ${response.url()}\n`);
  });

  let exitCode = 1;
  try {
    const spec = require(path.resolve(args.spec));
    if (typeof spec.run !== "function") throw new Error(`${args.spec} does not export run(ctx)`);
    const ctx = makeContext({
      args,
      profile,
      browser,
      context,
      page,
      facts: readJson(args.facts),
      receiptsIn: readJson(args.receipts),
      out,
      state,
    });
    await spec.run(ctx);
    result.status = "passed";
    exitCode = 0;
    await context.tracing.stop();
  } catch (error) {
    result.error = String(error && error.stack ? error.stack : error);
    try {
      await page.screenshot({ path: path.join(out, "failure.png"), animations: "disabled" });
    } catch (screenshotError) {
      consoleLog.write(`[cf-browser] failure screenshot failed: ${screenshotError.message}\n`);
    }
    await context.tracing.stop({ path: path.join(out, "trace.zip") });
  } finally {
    result.step = state.step;
    result.receipts = state.receipts;
    result.visuals = state.visuals;
    result.url = page.url();
    fs.writeFileSync(path.join(out, "result.json"), JSON.stringify(result, null, 2));
    await new Promise((resolve) => consoleLog.end(resolve));
    await browser.close();
  }
  return exitCode;
}

main().then(
  (code) => process.exit(code),
  (error) => {
    console.error(`cf-browser: ${error && error.stack ? error.stack : error}`);
    process.exit(1);
  },
);
