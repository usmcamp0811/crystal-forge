/**
 * Fixture → API route transformation.
 *
 * Pure data transformation — no Playwright dependency.
 * Transforms design-example fixture data (crystal-forge.fixtures.json)
 * into Dioxus API DTO response shapes.
 *
 * Shared by:
 *   - capture.js         (Playwright screenshot capture)
 *   - generate-fixture-routes.js  (pre-compute route JSON at build time)
 */
"use strict";

// ── helpers ────────────────────────────────────────────────────────────────
let _uuidSeed = 0;
function uuid(seed) {
  const s = seed !== undefined ? seed : _uuidSeed++;
  const h = (n, len) => n.toString(16).padStart(len, "0");
  return `${h(s, 8)}-${h(s & 0xffff, 4)}-4${h(s & 0xfff, 3)}-${h(s & 0x3fff | 0x8000, 4)}-${h(s, 12)}`;
}

// Fixture systems have `health: "healthy"` → Dioxus expects PascalCase "Healthy"
function healthStatus(val) {
  if (!val) return "Healthy";
  const v = String(val).toLowerCase();
  if (v === "healthy") return "Healthy";
  if (v === "warning") return "Warning";
  if (v === "critical") return "Critical";
  if (v === "offline") return "Offline";
  return "Healthy";
}

// Fixture systems have `deploymentState: "up-to-date"` → Dioxus expects snake_case "up_to_date"
function deploymentStatus(val) {
  if (!val) return "unknown";
  const v = String(val).toLowerCase().replace(/[\s_-]+/g, "_");
  if (v === "up_to_date" || v === "up-to-date") return "up_to_date";
  if (v === "behind") return "behind";
  if (v === "ahead") return "ahead";
  if (v === "never_deployed") return "never_deployed";
  if (v === "no_commits_available") return "no_commits_available";
  return "unknown";
}

// Fixture builds `status: "building"` → Dioxus expects PascalCase "Building"
function buildStatus(val) {
  if (!val) return "Queued";
  const v = String(val).toLowerCase();
  if (v === "building") return "Building";
  if (v === "queued") return "Queued";
  if (v === "complete") return "Complete";
  if (v === "failed") return "Failed";
  if (v === "cancelled") return "Cancelled";
  if (v === "cancelling") return "Cancelling";
  if (v === "idle") return "Idle";
  return "Queued";
}

// Fixture severity `"critical"` → PascalCase "Critical"
function capitalize(val) {
  if (!val) return "Medium";
  return val.charAt(0).toUpperCase() + val.slice(1).toLowerCase();
}

// Parse relative time strings like "2m ago", "1h ago" into ISO timestamp.
function parseTimeAgo(val, fallback) {
  if (!val) return fallback;
  if (/^\d{4}/.test(val)) return val;
  const m = val.match(/^(\d+)\s*(m|min|h|hr|d|day|s|sec|just now)/i);
  if (!m) return fallback;
  const n = parseInt(m[1], 10) || 1;
  const unit = m[2].toLowerCase();
  const d = new Date();
  if (unit === "s" || unit === "sec" || unit === "just now") { /* now */ }
  else if (unit === "m" || unit === "min") d.setMinutes(d.getMinutes() - n);
  else if (unit === "h" || unit === "hr") d.setHours(d.getHours() - n);
  else if (unit === "d" || unit === "day") d.setDate(d.getDate() - n);
  return d.toISOString();
}

function mapEvalStatus(s) {
  if (!s) return "queued";
  const v = String(s).toLowerCase();
  if (v === "queued" || v === "queue") return "queued";
  if (v === "evaluating" || v === "in_progress" || v === "in progress") return "evaluating";
  if (v === "complete" || v === "completed") return "complete";
  if (v === "failed") return "failed";
  if (v === "cancelled") return "cancelled";
  return s;
}

// Helper: build a predicate that matches an exact API path prefix.
function matchPath(path) {
  const fn = (url) => {
    const p = url.pathname;
    return p === path || p.startsWith(path + "/") || p.startsWith(path + "?");
  };
  fn._label = path;
  return fn;
}

// Match an exact path or prefix.
function matchPrefix(path) {
  const fn = (url) => url.pathname.startsWith(path);
  fn._label = path + "*";
  return fn;
}

/**
 * Transform fixture JSON into an array of route configs {pattern, body}.
 *
 * Each `pattern` is a function predicate (for Playwright) or a string path
 * (when the route map is flattened by generate-fixture-routes.js).
 * Each `body` is the DTO-shaped JSON response for that endpoint.
 */
function buildRoutes(fixtures) {
  const now = new Date().toISOString();

  // ══════════════════════════════════════════════════════════════════════════
  // DATA TRANSFORMERS — map fixture shapes → Dioxus API DTO shapes
  // ══════════════════════════════════════════════════════════════════════════

  // ── Systems (fixture → SystemSummary) ────────────────────────────────────
  const rawSystems   = (fixtures.systems || []).slice(0, 20);
  const systems      = rawSystems.map((s, i) => ({
    id:                         uuid(i),
    hostname:                   s.hostname || `system-${i}`,
    system_configuration_name:  null,
    environment:                s.environment || null,
    flake_id:                   null,
    primary_ip:                 null,
    health_status:              healthStatus(s.health),
    deployment_status:          deploymentStatus(s.deploymentState || s.status),
    pipeline_stage:             null,
    cve_counts: {
      critical:                 (s.cves && s.cves.critical) || 0,
      high:                     (s.cves && s.cves.high) || 0,
      medium:                   (s.cves && s.cves.medium) || 0,
      low:                      (s.cves && s.cves.low) || 0,
    },
    nixos_version:              s.nixosVersion || null,
    last_seen:                  parseTimeAgo(s.lastHeartbeat, now),
    deployment_policy:          s.deploymentPolicy || "default",
    fqdn:                       s.fqdn || null,
  }));

  // ── Build queue items (fixture builds.active → BuildQueueItem) ───────────
  const rawBuilds    = (fixtures.builds && fixtures.builds.active || []).slice(0, 10);
  const buildItems   = rawBuilds.map((b, i) => ({
    job_id:                     uuid(1000 + i),
    system_id:                  null,
    hostname:                   b.system || b.name || "",
    flake_name:                 b.flake || "",
    commit_hash:                b.commit || "",
    commit_message:             null,
    status:                     buildStatus(b.status),
    builder_name:               b.worker || null,
    queued_at:                  parseTimeAgo(b.queuedAt, now),
    started_at:                 null,
    elapsed_secs:               null,
    logs:                       null,
  }));

  // ── CVE items (fixture cves.list → CveListItem) ─────────────────────────
  const rawCves      = (fixtures.cves && fixtures.cves.list || []).slice(0, 20);
  const cveItems     = rawCves.map((c, i) => ({
    cve_id:                     c.id || `CVE-${i}`,
    cvss_v3_score:              c.cvss != null ? c.cvss : null,
    severity:                   capitalize(c.severity || "medium"),
    title:                      c.title || "",
    cvss_vector:                null,
    published_date:             null,
    exploited:                  c.exploited || false,
    package_name:               c.pkg || null,
    installed_version:          null,
    fixed_version:              c.fixedIn || null,
    fix_status:                 c.fix || "unknown",
    affected_count:             c.affectedCount || 0,
    affected_environments:      null,
    first_seen:                 null,
    last_seen:                  null,
    age_days:                   c.ageDays || 0,
    triage_status:              "unreviewed",
  }));

  // ── CVE summary from stats (fixture cves.stats → CveSummary) ────────────
  const cveStats     = (fixtures.cves && fixtures.cves.stats) || {};
  const cveSummary   = {
    critical: cveStats.critical || 0,
    high:     cveStats.high || 0,
    medium:   cveStats.medium || 0,
    low:      cveStats.low || 0,
  };

  // ── Dashboard summary (DashboardSummary) ─────────────────────────────────
  const dashSummary  = {
    fleet_health: {
      healthy:  systems.filter(s => s.health_status === "Healthy").length  || 7,
      warning:  systems.filter(s => s.health_status === "Warning").length  || 2,
      critical: systems.filter(s => s.health_status === "Critical").length || 1,
      offline:  systems.filter(s => s.health_status === "Offline").length  || 0,
    },
    deployment_status: {
      up_to_date:     systems.filter(s => s.deployment_status === "up_to_date").length || 6,
      behind:         systems.filter(s => s.deployment_status === "behind").length     || 2,
      never_deployed: 1,
      unknown:        1,
    },
    cve_summary:        cveSummary,
    total_systems:      systems.length || 10,
    active_builds:      buildItems.filter(b => b.status === "Building").length,
    build_queue: {
      building_count:   buildItems.filter(b => b.status === "Building").length,
      queued_count:     buildItems.filter(b => b.status === "Queued").length,
      timestamp:        now,
      items:            buildItems.slice(0, 3),
    },
    recent_deployments: [],
    timestamp:          now,
  };

  // ── Flake timelines (fixture flakes.registry + flakes.commits → Vec<FlakeTimeline>) ──
  const flakeReg      = (fixtures.flakes && fixtures.flakes.registry) || [];
  const flakeTimelines = flakeReg.map((f, i) => ({
    flake_id:   i,
    flake_name: f.name,
    repo_url:   f.url || "",
    commits:    ((fixtures.flakes && fixtures.flakes.commits && fixtures.flakes.commits[f.id]) || [])
      .slice(0, 5).map(c => ({
        id:                        0,
        hash:                      c.hash || c.id || "",
        message:                   c.message || "",
        author:                    c.author || "",
        committed_at:              parseTimeAgo(c.committedAt || c.at, now),
        system_count:              c.systemCount || 0,
        commits_behind:            0,
        systems:                   [],
        system_paths:              [],
        build_status:              null,
        evaluation_status:         null,
        evaluation_error_message:  null,
      })),
  }));

  // ── Flakes list (FlakeRegistryItem) ──────────────────────────────────────
  const flakeItems = flakeReg.map((f, i) => ({
    id:           i,
    name:         f.name || `flake-${i}`,
    repo_url:     f.url || "",
    branch:       f.branch || "main",
    build_scope:  "all",
    system_count: f.systemCount || 0,
  }));

  // ── Hardening (top services + fleet summary) ─────────────────────────────
  const hardData   = fixtures.hardening || {};
  const hardArr    = Array.isArray(hardData) ? hardData : Object.values(hardData);
  const hardTopServices = hardArr.slice(0, 10).map(h => ({
    service_name:           h.name || "unknown",
    affected_systems_count: 1,
    avg_score:              h.score || 0,
    min_score:              Math.max(0, (h.score || 0) - 20),
    max_score:              Math.min(100, (h.score || 0) + 10),
  }));
  const hardFleetSummary = {
    total_systems_scanned:              systems.length,
    avg_fleet_score:                    hardArr.length > 0
      ? hardArr.reduce((s, h) => s + (h.score || 0), 0) / hardArr.length
      : null,
    total_well_hardened_services:       hardArr.filter(h => (h.score || 0) >= 80).length,
    total_moderately_hardened_services: hardArr.filter(h => (h.score || 0) >= 50 && (h.score || 0) < 80).length,
    total_poorly_hardened_services:     hardArr.filter(h => (h.score || 0) >= 30 && (h.score || 0) < 50).length,
    total_vulnerable_services:          hardArr.filter(h => (h.score || 0) < 30).length,
    total_services_scanned:             hardArr.length,
    last_scan_completed:                now,
  };

  // ── Environments (EnvironmentSummary) ────────────────────────────────────
  const envs = (fixtures.environments || []).map((e, i) => ({
    id:           uuid(4000 + i),
    name:         e.name || `env-${i}`,
    description:  e.name || null,
    color_hex:    e.color || "#888",
    is_active:    true,
    system_count: systems.filter(s => s.environment === e.name).length || 2,
    rollup: {
      active_system_count: systems.filter(s => s.environment === e.name).length || 2,
      healthy:             systems.filter(s => s.environment === e.name && s.health_status === "Healthy").length || 1,
      warning:             systems.filter(s => s.environment === e.name && s.health_status === "Warning").length || 0,
      critical:            systems.filter(s => s.environment === e.name && s.health_status === "Critical").length || 0,
      offline:             0,
      cve_critical_high:   0,
      flakes:              [],
    },
  }));

  // ── Policies ─────────────────────────────────────────────────────────────
  const policies = (fixtures.policies || []).slice(0, 10).map((p, i) => ({
    id:                 p.id || uuid(3000 + i),
    name:               p.name || `policy-${i}`,
    description:        p.description || "",
    deployment_window:  null,
    is_default:         false,
    requires_approval:  false,
    created_at:         now,
    updated_at:         now,
  }));

  // ── Compliance bundles ───────────────────────────────────────────────────
  const compliance = fixtures.compliance || {};

  // ── Cache items ──────────────────────────────────────────────────────────
  const cacheItems = (fixtures.caches || []).slice(0, 10);

  // ── Scanning (ScanningStatsResponse) ─────────────────────────────────────
  const scanning = fixtures.scanning || {};
  const scanStats = scanning.stats || {};
  const scanningStatsResponse = {
    scanning:         scanStats.scanning || 0,
    queued:           scanStats.queued || 0,
    stale:            scanStats.stale || 0,
    never_scanned:    scanStats.unscanned || scanStats.never_scanned || 0,
    failed:           scanStats.failed || 0,
    coverage_percent: scanStats.coverage || scanStats.coverage_percent || 0,
  };

  // ── Builders ─────────────────────────────────────────────────────────────
  const builders = (fixtures.admin && fixtures.admin.builders || []).slice(0, 5);

  // ── Evaluations (eval queue + history) ───────────────────────────────────
  const evalActive  = (fixtures.evaluations && fixtures.evaluations.active || []).slice(0, 20);
  const evalHistory = (fixtures.evaluations && fixtures.evaluations.history || []).slice(0, 20);
  const evalQueue   = evalActive.map(e => ({
    id:             e.id || 0,
    flake_name:     e.flake || "",
    commit_hash:    e.commit || "",
    branch:         e.branch || "",
    status:         mapEvalStatus(e.status),
    system_count:   e.systemCount || 0,
    policy_pass:    e.policyPass || 0,
    policy_fail:    e.policyFail || 0,
    queued_at:      parseTimeAgo(e.startedAt, now),
    started_at:     parseTimeAgo(e.startedAt, now),
    completed_at:   parseTimeAgo(e.completedAt, null),
    duration_secs:  e.dur ? parseInt(e.dur) : null,
    can_cancel:     e.canCancel != null ? e.canCancel : true,
  }));
  const evalQueueFlat = evalQueue.map(e => ({
    id: e.id, flake_name: e.flake_name, commit_hash: e.commit_hash,
    status: e.status, system_count: e.system_count,
    policy_pass: e.policy_pass, policy_fail: e.policy_fail,
    queued_at: e.queued_at,
    started_at: e.started_at,
    completed_at: e.completed_at,
    duration_secs: e.duration_secs,
  }));

  // ── Admin config health ──────────────────────────────────────────────────
  const adminUsers = (fixtures.admin && fixtures.admin.users || []).slice(0, 20);

  // ── CVE dashboard summary (CveDashboardSummary) ──────────────────────────
  const cveDashSummary = {
    total_open:             cveStats.total || 0,
    severity:               cveSummary,
    affected_systems:       cveStats.systemsAffected || 0,
    new_cves_last_7_days:   cveStats.newToday || 0,
    oldest_cve_age_days:    cveStats.oldestDays || null,
  };

  // ── CVE fleet stats (CveFleetStats) ──────────────────────────────────────
  const cveFleetStats = {
    total_cves:             cveStats.total || 0,
    critical:               cveStats.critical || 0,
    high:                   cveStats.high || 0,
    medium:                 cveStats.medium || 0,
    low:                    cveStats.low || 0,
    exploited:              cveStats.exploited || 0,
    fixable:                cveStats.fixable || 0,
    environments_affected:  cveStats.envsAffected || 0,
    systems_affected:       cveStats.systemsAffected || 0,
    outstanding:            cveStats.outstanding || 0,
    accepted:               cveStats.accepted || 0,
    scheduled:              cveStats.scheduled || 0,
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
    { pattern: matchPath("/api/auth/whoami"),                              body: whoami },
    { pattern: matchPath("/api/auth/setup-status"),                        body: { requires_setup: false, allow_registration: false } },
    { pattern: matchPrefix("/api/auth/"),                                  body: whoami },

    // ── admin (specific before catch-all) ─────────────────────────────────────
    { pattern: matchPath("/api/v1/admin/config-health"),                   body: { status: "ok", issues: [] } },
    // Setup wizard: report dismissed + fully complete so the onboarding coach
    // overlay never renders. The coach uses hardcoded dark inline styles that
    // ignore the light theme, which otherwise pollutes light-mode screenshots.
    { pattern: matchPath("/api/v1/admin/setup-progress"), body: (() => {
        const doneStep = { complete: true, count: 1 };
        return {
          dismissed: true,
          agent_acknowledged: true,
          environment: doneStep,
          flake: doneStep,
          builder: doneStep,
          cache: doneStep,
          system: doneStep,
          all_required_complete: true,
        };
      })() },
    { pattern: matchPath("/api/v1/admin/users"),                           body: adminUsers },
    { pattern: matchPath("/api/v1/admin/server-info"),                     body: { version: "0.1.0-fixture", uptime_secs: 3600 } },
    { pattern: matchPath("/api/v1/admin/classification-config"),           body: { enabled: false, categories: [] } },
    { pattern: matchPath("/api/v1/admin/oidc-mappings"),                   body: [] },
    { pattern: matchPath("/api/v1/admin/audit-events"),                    body: { items: [], total: 0, page: 1, per_page: 50 } },
    { pattern: matchPath("/api/v1/admin/setup-wizard/dismiss"),            body: {} },
    { pattern: matchPath("/api/v1/admin/setup-wizard/agent-acknowledge"),  body: {} },
    { pattern: matchPrefix("/api/v1/admin/"),                              body: {} },

    // ── dashboard ─────────────────────────────────────────────────────────────
    { pattern: matchPath("/api/v1/dashboard/summary"),                     body: dashSummary },
    { pattern: matchPrefix("/api/v1/dashboard/"),                          body: dashSummary },

    // ── systems ───────────────────────────────────────────────────────────────
    { pattern: matchPrefix("/api/v1/systems"),                             body: { items: systems, total: systems.length, page: 1, per_page: 200 } },

    // ── environments ──────────────────────────────────────────────────────────
    { pattern: matchPath("/api/v1/environments/policies-map"),             body: [] },
    { pattern: matchPrefix("/api/v1/environments"),                        body: envs },

    // ── flakes ────────────────────────────────────────────────────────────────
    { pattern: matchPath("/api/v1/flakes/timelines"),                      body: flakeTimelines },
    { pattern: matchPath("/api/v1/flakes/sync"),                           body: {} },
    { pattern: matchPrefix("/api/v1/flakes/"),                             body: {} },
    { pattern: matchPrefix("/api/v1/flakes"),                              body: flakeItems },

    // ── builds ────────────────────────────────────────────────────────────────
    { pattern: matchPath("/api/v1/build-jobs/recent"),                     body: buildItems.slice(0, 5) },
    { pattern: matchPrefix("/api/v1/build-jobs"),                          body: { total: buildItems.length, page: 1, limit: 25, items: buildItems } },

    // ── evaluations (commits) ─────────────────────────────────────────────────
    { pattern: matchPath("/api/v1/commits/eval-queue"),                    body: evalQueueFlat },
    { pattern: matchPath("/api/v1/commits/eval-history"),                  body: { items: evalHistory.slice(0, 10).map(e => ({
      id: e.id || 0, flake_name: e.flake || "", commit_hash: e.commit || "",
      status: mapEvalStatus(e.status), system_count: e.systemCount || 0,
      queued_at: parseTimeAgo(e.startedAt, now), started_at: parseTimeAgo(e.startedAt, now),
      completed_at: parseTimeAgo(e.completedAt, null),
    })), total: 0, page: 1, limit: 25 } },
    { pattern: matchPrefix("/api/v1/commits/"),                            body: {} },

    // ── CVEs ──────────────────────────────────────────────────────────────────
    { pattern: matchPath("/api/v1/cves/summary"),                          body: cveDashSummary },
    { pattern: matchPath("/api/v1/cves/stats"),                            body: cveFleetStats },
    { pattern: matchPath("/api/v1/cves/top-systems"),                      body: [] },
    { pattern: matchPath("/api/v1/cves/scan-freshness"),                   body: [] },
    { pattern: matchPath("/api/v1/cves/vulnerabilities"),                  body: [] },
    { pattern: matchPath("/api/v1/cves/grouped"),                          body: { items: [], total: 0, page: 1, per_page: 50 } },
    { pattern: matchPath("/api/v1/cves/packages"),                         body: [] },
    { pattern: matchPrefix("/api/v1/cves"),                                body: cveItems },

    // ── policies ──────────────────────────────────────────────────────────────
    { pattern: matchPrefix("/api/v1/policies"),                            body: policies },
    { pattern: matchPrefix("/api/v1/deployment-policies"),                 body: { policies, total: policies.length, limit: 25, offset: 0 } },

    // ── compliance ────────────────────────────────────────────────────────────
    { pattern: matchPath("/api/v1/compliance/bundles"),                    body: [] },
    { pattern: matchPrefix("/api/v1/compliance/"),                         body: {} },

    // ── caches ────────────────────────────────────────────────────────────────
    { pattern: matchPrefix("/api/v1/caches"),                              body: cacheItems },
    { pattern: matchPrefix("/api/v1/cache-push-jobs"),                     body: { items: [], total: 0, page: 1, limit: 25 } },

    // ── builders ──────────────────────────────────────────────────────────────
    { pattern: matchPrefix("/api/v1/builders"),                            body: builders },

    // ── scanning ──────────────────────────────────────────────────────────────
    { pattern: matchPath("/api/v1/scanning/stats"),                        body: scanningStatsResponse },
    { pattern: matchPath("/api/v1/scanning/queue"),                        body: [] },
    { pattern: matchPath("/api/v1/scanning/systems"),                      body: [] },
    { pattern: matchPath("/api/v1/scanning/activity"),                     body: [] },
    { pattern: matchPath("/api/v1/scanning/schedule"),                     body: { interval_minutes: 1440, enabled: false } },
    { pattern: matchPrefix("/api/v1/scanning/"),                           body: {} },

    // ── hardening ─────────────────────────────────────────────────────────────
    { pattern: matchPath("/api/v1/hardening/summary"),                     body: hardFleetSummary },
    { pattern: matchPath("/api/v1/hardening/top-services"),                body: hardTopServices },
    { pattern: matchPath("/api/v1/hardening/systems"),                     body: [] },
    { pattern: matchPrefix("/api/v1/hardening/"),                          body: {} },

    // ── catch-all (MUST be last) ──────────────────────────────────────────────
    { pattern: matchPrefix("/api/v1/"),                                    body: {} },
  ];
}

module.exports = { buildRoutes };
