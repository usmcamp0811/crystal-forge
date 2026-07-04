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

// ── fixture → API route table ─────────────────────────────────────────────────
function buildRoutes(fixtures) {
  const now = new Date().toISOString();

  const systems     = (fixtures.systems || []).slice(0, 20);
  const envs        = fixtures.environments || [];
  const flakeReg    = fixtures.flakes?.registry || [];
  const flakeTimelines = flakeReg.map(f => ({
    flake_id: f.id, flake_name: f.name, repo_url: f.repo_url || "",
    commits: (fixtures.flakes?.commits?.[f.id] || []).slice(0, 5),
  }));
  const builds      = fixtures.builds || {};
  const buildItems  = (builds.queue || builds.items || []).slice(0, 10);
  const recentBuilds = (builds.recent || buildItems).slice(0, 5);
  const evals       = fixtures.evaluations || {};
  const cves        = fixtures.cves || {};
  const cveItems    = (cves.items || cves.list || []).slice(0, 20);
  const cveSummary  = cves.summary || {};
  const policies    = (fixtures.policies || []).slice(0, 10);
  const compliance  = fixtures.compliance || {};
  const cacheItems  = (fixtures.caches || []).slice(0, 10);
  const scanning    = fixtures.scanning || {};
  const admin       = fixtures.admin || {};
  const builders    = (admin.builders || []).slice(0, 5);

  const dashSummary = {
    fleet_health: {
      healthy:  systems.filter(s => s.health_status === "healthy").length  || 7,
      warning:  systems.filter(s => s.health_status === "warning").length  || 2,
      critical: systems.filter(s => s.health_status === "critical").length || 1,
      offline:  systems.filter(s => s.health_status === "offline").length  || 0,
    },
    deployment_status: {
      up_to_date:     systems.filter(s => s.deployment_status === "up_to_date").length || 6,
      behind:         systems.filter(s => s.deployment_status === "behind").length     || 2,
      never_deployed: 1, unknown: 1,
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

  // whoami: Role must be PascalCase ("Admin"), auth_mode snake_case ("local").
  const whoami = {
    is_authenticated: true,
    user: { id: "fixture-user", email: "fixture@example.com", display_name: "Fixture User" },
    roles: ["Admin"],
    auth_mode: "local",
  };

  return [
    // ── auth ──────────────────────────────────────────────────────────────────
    // whoami returns a valid session → WASM skips login redirect automatically.
    { pattern: "**/api/auth/whoami**",        body: whoami },
    { pattern: "**/api/auth/setup-status**",  body: { requires_setup: false, allow_registration: false } },
    { pattern: "**/api/auth/**",              body: whoami },

    // ── admin (config-health polled continuously — return ok to suppress banner) ──
    { pattern: "**/api/v1/admin/config-health**",   body: { status: "ok", issues: [] } },
    { pattern: "**/api/v1/admin/setup-progress**",  body: { complete: true } },
    { pattern: "**/api/v1/admin/users**",            body: admin.users || [] },
    { pattern: "**/api/v1/admin/**",                 body: {} },

    // ── data ──────────────────────────────────────────────────────────────────
    { pattern: "**/api/v1/dashboard/summary**",   body: dashSummary },
    { pattern: "**/api/v1/systems**",             body: { items: systems, total: systems.length, page: 1, per_page: 50 } },
    { pattern: "**/api/v1/environments**",        body: envs },
    { pattern: "**/api/v1/flakes/timelines**",    body: flakeTimelines },
    { pattern: "**/api/v1/flakes**",              body: flakeReg },
    { pattern: "**/api/v1/build-jobs/recent**",   body: recentBuilds },
    { pattern: "**/api/v1/build-jobs**",          body: { total: buildItems.length, page: 1, limit: 25, items: buildItems } },
    { pattern: "**/api/v1/commits/eval-queue**",  body: evals.queue  || [] },
    { pattern: "**/api/v1/commits/eval-history**",body: { items: (evals.history || []).slice(0,10), total: 0, page: 1, limit: 25 } },
    { pattern: "**/api/v1/commits/**",            body: {} },
    { pattern: "**/api/v1/cves/summary**",        body: cveSummary },
    { pattern: "**/api/v1/cves/stats**",          body: cveSummary },
    { pattern: "**/api/v1/cves**",                body: { items: cveItems, total: cveItems.length, page: 1, per_page: 50 } },
    { pattern: "**/api/v1/policies**",            body: policies },
    { pattern: "**/api/v1/deployment-policies**", body: policies },
    { pattern: "**/api/v1/compliance/**",         body: compliance.bundles || [] },
    { pattern: "**/api/v1/caches**",              body: cacheItems },
    { pattern: "**/api/v1/cache-push-jobs**",     body: [] },
    { pattern: "**/api/v1/builders**",            body: builders },
    { pattern: "**/api/v1/scanning/stats**",      body: scanning.stats || {} },
    { pattern: "**/api/v1/scanning/**",           body: scanning.queue || [] },
    { pattern: "**/api/v1/hardening/**",          body: {} },
    { pattern: "**/api/v1/**",                    body: [] },
  ];
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

    // Install all API intercepts
    for (const r of routes) {
      await page.route(r.pattern, route =>
        route.fulfill({ status: 200, contentType: "application/json",
                        body: JSON.stringify(r.body) }));
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
