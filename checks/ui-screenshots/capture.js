/**
 * UI Screenshots — fixture-driven Playwright capture
 *
 * Serves the pre-built Dioxus WASM bundle, intercepts all /api/v1/ calls
 * with fixture JSON, and screenshots each view in dark + light mode.
 * No backend, no database, no network required.
 *
 * Usage:
 *   node capture.js <webUiPublicDir> <fixturesJson> <outputDir>
 */
"use strict";

const { chromium } = require("playwright");
const fs   = require("fs");
const path = require("path");
const http = require("http");

// ── args ─────────────────────────────────────────────────────────────────────
const publicDir  = process.argv[2];
const fixturesPath = process.argv[3];
const outputDir  = process.argv[4] || "/tmp/ui-screenshots";

if (!publicDir || !fixturesPath) {
  console.error("usage: node capture.js <webUiPublicDir> <fixturesJson> <outputDir>");
  process.exit(2);
}

// ── static SPA server ─────────────────────────────────────────────────────────
function startServer(dir, port) {
  const server = http.createServer((req, res) => {
    // SPA: strip query string, fall back to index.html
    let urlPath = req.url.split("?")[0];
    let filePath = path.join(dir, urlPath);
    if (!fs.existsSync(filePath) || fs.statSync(filePath).isDirectory()) {
      filePath = path.join(dir, "index.html");
    }
    const ext = path.extname(filePath);
    const mime = {
      ".html": "text/html", ".js": "application/javascript",
      ".wasm": "application/wasm", ".css": "text/css",
      ".ico": "image/x-icon", ".png": "image/png", ".svg": "image/svg+xml",
    }[ext] || "application/octet-stream";
    try {
      const data = fs.readFileSync(filePath);
      res.writeHead(200, { "Content-Type": mime });
      res.end(data);
    } catch {
      res.writeHead(404);
      res.end("not found");
    }
  });
  server.listen(port);
  return server;
}

// ── fixture → API adapters ────────────────────────────────────────────────────
function buildRoutes(fixtures) {
  const now = new Date().toISOString();

  // systems
  const systems = (fixtures.systems || []).slice(0, 20);
  const systemsPage = { items: systems, total: systems.length, page: 1, per_page: 50 };

  // environments
  const environments = fixtures.environments || [];

  // flakes
  const flakeRegistry = (fixtures.flakes?.registry || []);
  const flakeTimelines = flakeRegistry.map(f => ({
    flake_id: f.id, flake_name: f.name, repo_url: f.repo_url || "",
    commits: (fixtures.flakes?.commits?.[f.id] || []).slice(0, 5),
  }));

  // builds
  const builds = fixtures.builds || {};
  const buildItems = (builds.queue || builds.items || []).slice(0, 10);
  const recentBuilds = (builds.recent || buildItems).slice(0, 5);
  const buildQueuePage = { total: buildItems.length, page: 1, limit: 25, items: buildItems };

  // evaluations
  const evals = fixtures.evaluations || {};
  const evalQueue = (evals.queue || []).slice(0, 10);
  const evalHistory = { items: (evals.history || []).slice(0, 10), total: 0, page: 1, limit: 25 };

  // cves
  const cves = fixtures.cves || {};
  const cveItems = (cves.items || cves.list || []).slice(0, 20);
  const cveSummary = cves.summary || { critical: 0, high: 0, medium: 0, low: 0 };
  const cveStats = cves.stats || cveSummary;

  // policies
  const policies = (fixtures.policies || []).slice(0, 10);

  // compliance
  const compliance = fixtures.compliance || {};
  const complianceBundles = (compliance.bundles || []).slice(0, 5);

  // caches
  const cacheItems = (fixtures.caches || []).slice(0, 10);

  // scanning
  const scanning = fixtures.scanning || {};

  // builders
  const admin = fixtures.admin || {};
  const builders = (admin.builders || []).slice(0, 5);

  // hardening
  const hardening = fixtures.hardening || {};

  // dashboard summary assembled from fixture pieces
  const dashboardSummary = {
    fleet_health: {
      healthy: systems.filter(s => s.health_status === "healthy").length || 7,
      warning: systems.filter(s => s.health_status === "warning").length || 2,
      critical: systems.filter(s => s.health_status === "critical").length || 1,
      offline: systems.filter(s => s.health_status === "offline").length || 0,
    },
    deployment_status: {
      up_to_date: systems.filter(s => s.deployment_status === "up_to_date").length || 6,
      behind: systems.filter(s => s.deployment_status === "behind").length || 2,
      never_deployed: 1,
      unknown: 1,
    },
    cve_summary: cveSummary,
    total_systems: systems.length || 10,
    active_builds: buildItems.filter(b => b.status === "building").length,
    build_queue: {
      building_count: buildItems.filter(b => b.status === "building").length,
      queued_count:   buildItems.filter(b => b.status === "queued").length,
      timestamp: now,
      items: buildItems.slice(0, 3),
    },
    recent_deployments: (builds.recent_deployments || []).slice(0, 5),
    timestamp: now,
  };

  return [
    // auth — skip whoami entirely (ui_check_auth=1 handles this in the WASM)
    // dashboard
    { pattern: "**/api/v1/dashboard/summary**", body: dashboardSummary },
    // systems
    { pattern: "**/api/v1/systems**",           body: systemsPage },
    // environments
    { pattern: "**/api/v1/environments**",      body: environments },
    // flakes
    { pattern: "**/api/v1/flakes/timelines**",  body: flakeTimelines },
    { pattern: "**/api/v1/flakes**",            body: flakeRegistry },
    // builds
    { pattern: "**/api/v1/build-jobs/recent**", body: recentBuilds },
    { pattern: "**/api/v1/build-jobs**",        body: buildQueuePage },
    // evaluations
    { pattern: "**/api/v1/commits/eval-queue**",    body: evalQueue },
    { pattern: "**/api/v1/commits/eval-history**",  body: evalHistory },
    // cves
    { pattern: "**/api/v1/cves/summary**",      body: cveSummary },
    { pattern: "**/api/v1/cves/stats**",        body: cveStats },
    { pattern: "**/api/v1/cves**",              body: { items: cveItems, total: cveItems.length, page: 1, per_page: 50 } },
    // policies
    { pattern: "**/api/v1/policies**",          body: policies },
    { pattern: "**/api/v1/deployment-policies**", body: policies },
    // compliance
    { pattern: "**/api/v1/compliance/**",       body: complianceBundles },
    // caches
    { pattern: "**/api/v1/caches**",            body: cacheItems },
    { pattern: "**/api/v1/cache-push-jobs**",   body: [] },
    // builders
    { pattern: "**/api/v1/builders**",          body: builders },
    // scanning
    { pattern: "**/api/v1/scanning/stats**",    body: scanning.stats || {} },
    { pattern: "**/api/v1/scanning/**",         body: scanning.queue || [] },
    // hardening
    { pattern: "**/api/v1/hardening/**",        body: hardening.summary || {} },
    { pattern: "**/api/v1/hardening**",         body: hardening.summary || {} },
    // admin
    { pattern: "**/api/v1/admin/config-health**", body: { status: "ok" } },
    { pattern: "**/api/v1/admin/setup-progress**", body: { complete: true } },
    { pattern: "**/api/v1/admin/users**",       body: admin.users || [] },
    { pattern: "**/api/v1/admin/**",            body: {} },
  ];
}

// ── views to capture ──────────────────────────────────────────────────────────
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
  const baseUrl = `http://localhost:${PORT}`;

  const browser = await chromium.launch({
    args: [
      "--no-sandbox",
      "--disable-dev-shm-usage",
      "--disable-setuid-sandbox",
    ],
  });

  const results = [];

  for (const theme of THEMES) {
    const context = await browser.newContext({
      viewport: { width: 1920, height: 1080 },
      timezoneId: "UTC",
      locale: "en-US",
    });
    const page = await context.newPage();

    // Install API route intercepts
    for (const r of routes) {
      await page.route(r.pattern, route =>
        route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(r.body) })
      );
    }

    for (const view of VIEWS) {
      const name = `${view.name}--${theme}`;
      const url  = `${baseUrl}${view.route}?ui_check_auth=1`;
      let ok = true, error = null;

      try {
        // Pre-seed theme in localStorage before navigating
        await page.goto(`${baseUrl}/?ui_check_auth=1`, { timeout: 15000, waitUntil: "domcontentloaded" });
        await page.evaluate(t => localStorage.setItem("cf.ui.theme", t), theme);

        await page.goto(url, { timeout: 15000, waitUntil: "networkidle" });
        // Wait for the main content area to appear
        await page.waitForSelector(".app, #main, main, .content, body", { timeout: 10000 });
        await page.waitForTimeout(600);

        // Force theme attribute in case WASM didn't pick up localStorage yet
        await page.evaluate(t => {
          localStorage.setItem("cf.ui.theme", t);
          document.documentElement.setAttribute("data-theme", t);
        }, theme);
        await page.waitForTimeout(200);

        const outPath = path.join(outputDir, `${name}.png`);
        await page.screenshot({ path: outPath, fullPage: false });
        console.log(`  OK  ${name}`);
      } catch (err) {
        ok = false;
        error = err.message.split("\n")[0];
        console.error(`  FAIL ${name}: ${error}`);
        try {
          await page.screenshot({ path: path.join(outputDir, `${name}.png`) });
        } catch (_) {}
      }
      results.push({ name, view: view.name, theme, ok, error });
    }

    await context.close();
  }

  await browser.close();
  server.close();

  fs.writeFileSync(path.join(outputDir, "results.json"), JSON.stringify({ results }, null, 2));

  const ok  = results.filter(r => r.ok).length;
  const bad = results.filter(r => !r.ok);
  console.log(`\nDone: ${ok}/${results.length} captured`);
  if (bad.length) {
    bad.forEach(r => console.log(`  FAIL ${r.name}: ${r.error}`));
    process.exit(1);
  }
}

main().catch(err => {
  console.error("Fatal:", err.message);
  process.exit(1);
});
