/**
 * UI Screenshots — fixture-driven Playwright capture
 *
 * Serves the pre-built Dioxus WASM bundle, intercepts all /api/ calls with
 * fixture JSON (including auth/whoami so the WASM skips login), and
 * screenshots each view in dark + light mode.
 *
 * No backend, no database, no Rust changes needed.
 *
 * Usage:
 *   node capture.js <webUiPublicDir> <fixturesJson> <outputDir>
 */
"use strict";

const { chromium } = require("playwright");
const fs   = require("fs");
const path = require("path");
const http = require("http");

const { buildRoutes } = require("./routes.js");

const publicDir    = process.argv[2];
const fixturesPath = process.argv[3];
const outputDir    = process.argv[4] || "/tmp/ui-screenshots";

if (!publicDir || !fixturesPath) {
  console.error("usage: node capture.js <webUiPublicDir> <fixturesJson> <outputDir>");
  process.exit(2);
}

// ── SPA server ────────────────────────────────────────────────────────────────
function startServer(dir, port) {
  const MIME = {
    ".html": "text/html", ".js": "application/javascript",
    ".wasm": "application/wasm", ".css": "text/css",
    ".ico": "image/x-icon", ".png": "image/png", ".svg": "image/svg+xml",
  };
  const server = http.createServer((req, res) => {
    let fp = path.join(dir, req.url.split("?")[0]);
    if (!fs.existsSync(fp) || fs.statSync(fp).isDirectory())
      fp = path.join(dir, "index.html");
    const mime = MIME[path.extname(fp)] || "application/octet-stream";
    res.writeHead(200, { "Content-Type": mime });
    res.end(fs.readFileSync(fp));
  });
  server.listen(port);
  return server;
}

// ── views ─────────────────────────────────────────────────────────────────────
const VIEWS = [
  { name: "dashboard",    route: "/" },
  { name: "systems",      route: "/systems" },
  { name: "builds",       route: "/builds" },
  { name: "evaluations",  route: "/evaluations" },
  { name: "flakes",       route: "/flakes" },
  { name: "environments", route: "/environments" },
  { name: "caches",       route: "/caches" },
  { name: "builders",     route: "/builders" },
  { name: "policies",     route: "/deployment-policies" },
  { name: "compliance",   route: "/compliance" },
  { name: "cves",         route: "/cves" },
  { name: "scanning",     route: "/scanning" },
  { name: "admin",        route: "/admin" },
];
const THEMES = ["dark", "light"];
const PORT   = 19876;

// ── main ──────────────────────────────────────────────────────────────────────
async function main() {
  const fixtures = JSON.parse(fs.readFileSync(fixturesPath, "utf8"));
  const routes   = buildRoutes(fixtures);
  fs.mkdirSync(outputDir, { recursive: true });

  const server = startServer(publicDir, PORT);

  const browser = await chromium.launch({
    args: ["--no-sandbox", "--disable-dev-shm-usage", "--disable-setuid-sandbox"],
  });

  const results = [];

  for (const theme of THEMES) {
    const context = await browser.newContext({
      viewport: { width: 1920, height: 1080 },
      timezoneId: "UTC",
      locale: "en-US",
      storageState: {
        cookies: [],
        origins: [{ origin: `http://localhost:${PORT}`, localStorage: [
          { name: "cf.ui.theme", value: theme },
        ]}],
      },
    });
    const page = await context.newPage();

    // Install all API intercepts.
    // CRITICAL: iterate in REVERSE order so the catch-all (last in array) is
    // registered FIRST. Playwright calls matching handlers in LIFO order (last
    // registered runs first), so specific patterns must be registered last.
    for (const r of routes.slice().reverse()) {
      const pattern = r.pattern;
      const label = typeof pattern === "string" ? pattern :
                    typeof pattern === "function" ? (pattern._label || "fn") :
                    pattern.source;
      try {
        await page.route(pattern, async (route) => {
          const req = route.request();
          const url = req.url();
          const pathPart = url.replace(/^.*\/\/[^/]+/, '');
          const bodyStr = JSON.stringify(r.body);
          try {
            await route.fulfill({ status: 200, contentType: "application/json",
                                  body: bodyStr });
            if (!url.includes('/api/auth/')) {
              console.log(`    ROUTE ${label.padEnd(50)} ${pathPart}`);
            }
          } catch (err) {
            console.error(`    ROUTE FAIL ${label} ${pathPart}: ${err.message}`);
            await route.fallback().catch(() => {});
          }
        });
      } catch (regErr) {
        console.error(`    ROUTE REGISTRATION FAIL ${label}: ${regErr.message}`);
      }
    }

    for (const view of VIEWS) {
      const name = `${view.name}--${theme}`;
      let ok = true, error = null;
      try {
        await page.goto(`http://localhost:${PORT}${view.route}`,
                        { timeout: 30000, waitUntil: "domcontentloaded" });
        // Wait until we're no longer on the login page
        await page.waitForFunction(
          () => !window.location.pathname.startsWith("/login"),
          { timeout: 15000 }
        );
        await page.waitForTimeout(1000);
        const outPath = path.join(outputDir, `${name}.png`);
        await page.screenshot({ path: outPath });
        console.log(`  OK  ${name}`);
      } catch (err) {
        ok = false;
        error = err.message.split("\n")[0];
        console.error(`  FAIL ${name}: ${error}`);
        try { await page.screenshot({ path: path.join(outputDir, `${name}.png`) }); }
        catch (_) {}
      }
      results.push({ name, view: view.name, theme, ok, error });
    }
    await context.close();
  }

  await browser.close();
  server.close();

  fs.writeFileSync(path.join(outputDir, "results.json"),
                   JSON.stringify({ results }, null, 2));

  const ok  = results.filter(r => r.ok).length;
  const bad = results.filter(r => !r.ok);
  console.log(`\nDone: ${ok}/${results.length} captured`);
  if (bad.length) {
    bad.forEach(r => console.log(`  FAIL ${r.name}: ${r.error}`));
    process.exit(1);
  }
}

main().catch(err => { console.error("Fatal:", err.message); process.exit(1); });
