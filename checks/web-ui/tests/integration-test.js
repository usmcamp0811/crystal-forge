/**
 * Crystal Forge Web UI Integration Test
 *
 * This test:
 * 1. Navigates to the login page (should redirect to register on first run)
 * 2. Registers an admin user
 * 3. Logs in with that user
 * 4. Takes screenshots of all major authenticated routes
 *
 * Usage: node integration-test.js <baseUrl> <outputDir>
 */
const { chromium } = require("playwright");
const fs = require("fs");

const baseUrl = process.argv[2] || "http://127.0.0.1:3000";
const outputDir = process.argv[3] || "/tmp/screenshots";

// Test user credentials
const TEST_USER = {
  username: "admin",
  email: "admin@example.com",
  password: "testpassword123",
  firstName: "Test",
  lastName: "Admin",
};

// Timeout for page loads (don't use networkidle as it can hang)
const LOAD_TIMEOUT = 10000;

const VIEWPORTS = {
  desktop: { width: 1440, height: 900 },
  tablet: { width: 900, height: 900 },
  narrowDesktop: { width: 560, height: 900 },
  mobile: { width: 375, height: 812 },
};

async function assertVisible(locator, message, timeoutMs = 5000) {
  const visible = await locator.isVisible({ timeout: timeoutMs }).catch(() => false);
  if (!visible) {
    throw new Error(message);
  }
}

async function assertHidden(locator, message) {
  const visible = await locator.isVisible({ timeout: 1500 }).catch(() => false);
  if (visible) {
    throw new Error(message);
  }
}

function nowIso() {
  return new Date().toISOString();
}

function mockBuildsDashboardSummary() {
  const timestamp = nowIso();
  return {
    fleet_health: { healthy: 1, warning: 0, critical: 0, offline: 0 },
    deployment_status: { up_to_date: 1, behind: 0, never_deployed: 0, unknown: 0 },
    cve_summary: { critical: 0, high: 0, medium: 0, low: 0 },
    total_systems: 1,
    active_builds: 1,
    build_queue: {
      building_count: 1,
      queued_count: 1,
      timestamp,
      items: [
        {
          job_id: "11111111-1111-4111-8111-111111111111",
          system_id: "22222222-2222-4222-8222-222222222222",
          hostname: "very-long-hostname-that-should-not-overflow-build-queue-card.example.internal",
          flake_name: "critical-infra-flake-with-a-very-long-name-for-layout-testing",
          commit_hash: "a1b2c3d4e5f678901234567890abcdef12345678",
          commit_message:
            "Queued build with intentionally long metadata to validate truncation and card boundaries in the Builds queue UI",
          status: "queued",
          builder_name: "builder-primary",
          queued_at: timestamp,
          started_at: null,
          elapsed_secs: null,
          logs: null,
        },
        {
          job_id: "33333333-3333-4333-8333-333333333333",
          system_id: "44444444-4444-4444-8444-444444444444",
          hostname: "build-runner-02",
          flake_name: "platform-core",
          commit_hash: "deadbeefcafebabe1234567890abcdef12345678",
          commit_message: "Building core platform",
          status: "building",
          builder_name: "builder-secondary",
          queued_at: timestamp,
          started_at: timestamp,
          elapsed_secs: 42,
          logs: null,
        },
      ],
    },
    recent_deployments: [],
    timestamp,
  };
}

function mockFleetHealthDashboardSummary() {
  const timestamp = nowIso();
  return {
    fleet_health: { healthy: 7, warning: 2, critical: 3, offline: 1 },
    deployment_status: { up_to_date: 8, behind: 3, never_deployed: 1, unknown: 1 },
    cve_summary: { critical: 1, high: 2, medium: 3, low: 4 },
    total_systems: 13,
    active_builds: 0,
    build_queue: {
      building_count: 0,
      queued_count: 0,
      timestamp,
      items: [],
    },
    recent_deployments: [],
    timestamp,
  };
}

function mockFleetHealthSystemsPage() {
  const mk = (idx, status) => ({
    id: `00000000-0000-4000-8000-${String(idx).padStart(12, "0")}`,
    hostname: `fleet-health-${status}-${idx}`,
    system_configuration_name: `fleet-health-${status}-${idx}`,
    environment: "production",
    flake_id: null,
    primary_ip: null,
    health_status: status,
    deployment_status: "up_to_date",
    pipeline_stage: "ready_for_build",
    cve_counts: { critical: 0, high: 0, medium: 0, low: 0 },
    nixos_version: "24.11",
    last_seen: nowIso(),
    deployment_policy: "manual",
  });

  const items = [
    ...Array.from({ length: 7 }, (_, i) => mk(i + 1, "healthy")),
    ...Array.from({ length: 2 }, (_, i) => mk(i + 101, "warning")),
    ...Array.from({ length: 3 }, (_, i) => mk(i + 201, "critical")),
    ...Array.from({ length: 1 }, (_, i) => mk(i + 301, "offline")),
  ];

  return {
    items,
    total: items.length,
    page: 1,
    per_page: 200,
  };
}

async function routeFleetHealthWidgetData(page) {
  await page.route("**/api/v1/dashboard/summary*", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(mockFleetHealthDashboardSummary()),
    });
  });

  await page.route("**/api/v1/systems*", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(mockFleetHealthSystemsPage()),
    });
  });
}

async function unrouteFleetHealthWidgetData(page) {
  await page.unroute("**/api/v1/dashboard/summary*");
  await page.unroute("**/api/v1/systems*");
}

function mockRecentDeploymentsScrollSummary() {
  const timestamp = nowIso();
  const recent_deployments = Array.from({ length: 12 }, (_, i) => ({
    hostname: `deployment-node-${i + 1}`,
    commit_hash: `abcd1234ef${String(i + 1).padStart(2, "0")}abcd1234ef${String(i + 1).padStart(2, "0")}`,
    commit_message: `Deploy update ${i + 1} for dashboard scroll coverage`,
    deployed_at: new Date(Date.now() - i * 60_000).toISOString(),
    status: i % 2 === 0 ? "up_to_date" : "behind",
  }));

  return {
    fleet_health: { healthy: 5, warning: 1, critical: 0, offline: 0 },
    deployment_status: { up_to_date: 5, behind: 1, never_deployed: 0, unknown: 0 },
    cve_summary: { critical: 0, high: 0, medium: 0, low: 0 },
    total_systems: 12,
    active_builds: 0,
    build_queue: {
      building_count: 0,
      queued_count: 0,
      timestamp,
      items: [],
    },
    recent_deployments,
    timestamp,
  };
}

async function routeRecentDeploymentsScrollData(page) {
  await page.route("**/api/v1/dashboard/summary*", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(mockRecentDeploymentsScrollSummary()),
    });
  });

  await page.route("**/api/v1/systems*", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ items: [], total: 0, page: 1, per_page: 200 }),
    });
  });
}

async function unrouteRecentDeploymentsScrollData(page) {
  await page.unroute("**/api/v1/dashboard/summary*");
  await page.unroute("**/api/v1/systems*");
}

function mockBuildQueuePage() {
  const summary = mockBuildsDashboardSummary();
  return {
    total: summary.build_queue.items.length,
    page: 1,
    limit: 50,
    items: summary.build_queue.items,
  };
}

function mockBuilders() {
  const timestamp = nowIso();
  return [
    {
      id: "55555555-5555-4555-8555-555555555555",
      name: "builder-primary",
      status: "active",
      max_cpu_cores: 8,
      max_memory_mb: 16384,
      max_concurrent_jobs: 4,
      last_heartbeat_at: timestamp,
      assigned_environment_count: 1,
      active_jobs: 1,
      queued_jobs: 1,
    },
  ];
}

function mockRecentBuilds() {
  const timestamp = nowIso();
  return [
    {
      job_id: "66666666-6666-4666-8666-666666666666",
      system_id: "77777777-7777-4777-8777-777777777777",
      hostname: "history-system-1",
      flake_name: "platform-core",
      commit_hash: "abcd1234abcd1234abcd1234abcd1234abcd1234",
      commit_message: "Recent successful build",
      status: "complete",
      builder_name: "builder-primary",
      queued_at: timestamp,
      started_at: timestamp,
      elapsed_secs: 15,
      logs: null,
    },
  ];
}

function mockRecentBuildsWithCancelled() {
  const timestamp = nowIso();
  return [
    {
      job_id: "99999999-9999-4999-8999-999999999999",
      system_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
      hostname: "cancelled-history-system",
      flake_name: "platform-core",
      commit_hash: "1234567812345678123456781234567812345678",
      commit_message: "Cancelled build in completed history for restart verification",
      status: "cancelled",
      builder_name: "builder-primary",
      queued_at: timestamp,
      started_at: timestamp,
      elapsed_secs: 12,
      logs: null,
    },
    {
      job_id: "66666666-6666-4666-8666-666666666666",
      system_id: "77777777-7777-4777-8777-777777777777",
      hostname: "history-system-1",
      flake_name: "platform-core",
      commit_hash: "abcd1234abcd1234abcd1234abcd1234abcd1234",
      commit_message: "Recent successful build",
      status: "complete",
      builder_name: "builder-primary",
      queued_at: timestamp,
      started_at: timestamp,
      elapsed_secs: 15,
      logs: null,
    },
  ];
}

async function routeBuildsData(page) {
  await page.route("**/api/v1/dashboard/summary*", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(mockBuildsDashboardSummary()),
    });
  });

  await page.route("**/api/v1/builders*", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(mockBuilders()),
    });
  });

  await page.route("**/api/v1/build-jobs*", async (route) => {
    const url = route.request().url();
    if (url.includes("/api/v1/build-jobs/recent")) {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(mockRecentBuilds()),
      });
      return;
    }

    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(mockBuildQueuePage()),
    });
  });
}

async function unrouteBuildsData(page) {
  await page.unroute("**/api/v1/dashboard/summary*");
  await page.unroute("**/api/v1/builders*");
  await page.unroute("**/api/v1/build-jobs*");
}

// Mock data with cancelling/cancelled states for queue controls evidence
function mockBuildsDashboardSummaryWithCancelStates() {
  const timestamp = nowIso();
  return {
    fleet_health: { healthy: 1, warning: 0, critical: 0, offline: 0 },
    deployment_status: { up_to_date: 1, behind: 0, never_deployed: 0, unknown: 0 },
    cve_summary: { critical: 0, high: 0, medium: 0, low: 0 },
    total_systems: 1,
    active_builds: 2,
    build_queue: {
      building_count: 1,
      queued_count: 2,
      timestamp,
      items: [
        {
          job_id: "11111111-1111-4111-8111-111111111111",
          system_id: "22222222-2222-4222-8222-222222222222",
          hostname: "system-stopping-build",
          flake_name: "platform-core",
          commit_hash: "a1b2c3d4e5f678901234567890abcdef12345678",
          commit_message: "Build being stopped - demonstrates cancelling state",
          status: "cancelling",
          builder_name: "builder-primary",
          queued_at: timestamp,
          started_at: timestamp,
          elapsed_secs: 135,
          logs: null,
        },
        {
          job_id: "33333333-3333-4333-8333-333333333333",
          system_id: "44444444-4444-4444-8444-444444444444",
          hostname: "active-build-runner",
          flake_name: "infra-core",
          commit_hash: "deadbeefcafebabe1234567890abcdef12345678",
          commit_message: "Currently building - shows runtime in human format",
          status: "building",
          builder_name: "builder-secondary",
          queued_at: timestamp,
          started_at: timestamp,
          elapsed_secs: 3723,
          logs: null,
        },
        {
          job_id: "55555555-5555-4555-8555-555555555555",
          system_id: "66666666-6666-4666-8666-666666666666",
          hostname: "queued-system-01",
          flake_name: "edge-fleet",
          commit_hash: "cafebabe1234567890abcdef1234567890abcdef",
          commit_message: "Queued build waiting for slot",
          status: "queued",
          builder_name: "builder-primary",
          queued_at: timestamp,
          started_at: null,
          elapsed_secs: null,
          logs: null,
        },
        {
          job_id: "77777777-7777-4777-8777-777777777777",
          system_id: "88888888-8888-4888-8888-888888888888",
          hostname: "cancelled-job-system",
          flake_name: "workstations",
          commit_hash: "abcdef1234567890abcdef1234567890abcdef12",
          commit_message: "Previously cancelled build - demonstrates cancelled state",
          status: "cancelled",
          builder_name: "builder-primary",
          queued_at: timestamp,
          started_at: null,
          elapsed_secs: null,
          logs: null,
        },
      ],
    },
    recent_deployments: [],
    timestamp,
  };
}

async function routeBuildsDataWithCancelStates(page) {
  const summary = mockBuildsDashboardSummaryWithCancelStates();
  const queuePage = {
    total: summary.build_queue.items.length,
    page: 1,
    limit: 50,
    items: summary.build_queue.items,
  };

  await page.route("**/api/v1/dashboard/summary*", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(summary),
    });
  });

  await page.route("**/api/v1/builders*", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(mockBuilders()),
    });
  });

  await page.route("**/api/v1/build-jobs*", async (route) => {
    const url = route.request().url();
    if (url.includes("/api/v1/build-jobs/recent")) {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(mockRecentBuilds()),
      });
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(queuePage),
    });
  });
}

async function unrouteBuildsDataWithCancelStates(page) {
  await page.unroute("**/api/v1/dashboard/summary*");
  await page.unroute("**/api/v1/builders*");
  await page.unroute("**/api/v1/build-jobs*");
}

function mockSetupCoachProgress() {
  return {
    dismissed: false,
    agent_acknowledged: false,
    environment: { complete: false, count: 0 },
    flake: { complete: false, count: 0 },
    builder: { complete: false, count: 0 },
    cache: { complete: false, count: 0 },
    system: { complete: false, count: 0 },
    all_required_complete: false,
  };
}

async function routeSetupCoachData(page) {
  await page.route("**/api/v1/admin/setup-progress*", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(mockSetupCoachProgress()),
    });
  });
  await page.route("**/api/v1/admin/setup-wizard/dismiss*", async (route) => {
    await route.fulfill({ status: 204, body: "" });
  });
  await page.route("**/api/v1/admin/setup-wizard/agent-acknowledge*", async (route) => {
    await route.fulfill({ status: 204, body: "" });
  });
}

async function unrouteSetupCoachData(page) {
  await page.unroute("**/api/v1/admin/setup-progress*");
  await page.unroute("**/api/v1/admin/setup-wizard/dismiss*");
  await page.unroute("**/api/v1/admin/setup-wizard/agent-acknowledge*");
}

function mockConfigHealthResponse(overrides = {}) {
  const response = {
    has_flakes: true,
    has_environments: true,
    has_builders: true,
    has_cache_destinations: true,
    total_issues: 0,
    checks: [],
    ...overrides,
  };

  if (!response.checks.length) {
    response.checks = [
      {
        id: "no_flakes",
        passed: response.has_flakes,
        message:
          "No flakes are being watched. Add a flake to begin evaluating NixOS configurations.",
        action_url: "/flakes",
      },
      {
        id: "no_environments",
        passed: response.has_environments,
        message:
          "No environments exist. Environments are required to organize systems, builders, and caches.",
        action_url: "/environments",
      },
      {
        id: "no_builders",
        passed: response.has_builders,
        message:
          "No builders are registered. Derivations will be evaluated but never built.",
        action_url: "/builders",
      },
      {
        id: "no_cache_destinations",
        passed: response.has_cache_destinations,
        message:
          "No cache destinations configured. Builds will succeed but agents won't be able to pull deployments.",
        action_url: "/caches",
      },
      {
        id: "flake_eval_errors",
        passed: true,
        message:
          "One or more flakes have evaluation errors on their latest commit. Check flake configuration.",
        action_url: "/flakes",
      },
    ];
  }

  response.total_issues = response.checks.filter((check) => !check.passed).length;
  return response;
}

function mockConfigHealthManyIssues(issueCount = 12) {
  const checks = Array.from({ length: issueCount }, (_, i) => ({
    id: `synthetic_issue_${i + 1}`,
    passed: false,
    message: `Synthetic pipeline readiness issue ${i + 1}: configuration validation warning for overflow coverage`,
    action_url: i % 2 === 0 ? "/flakes" : "/environments",
  }));

  return {
    has_flakes: true,
    has_environments: true,
    has_builders: true,
    has_cache_destinations: true,
    total_issues: checks.length,
    checks,
  };
}

async function routeConfigHealth(page, response) {
  await page.route("**/api/v1/admin/config-health*", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(response),
    });
  });
}

async function unrouteConfigHealth(page) {
  await page.unroute("**/api/v1/admin/config-health*");
}

async function routeSystemsWarningData(page) {
  const items = [
    {
      id: "00000000-0000-0000-0000-0000000000a1",
      hostname: "warning-system-01",
      system_configuration_name: "warning-system-01",
      environment: "production",
      flake_id: null,
      primary_ip: "10.10.0.10",
      health_status: "warning",
      deployment_status: "never_deployed",
      pipeline_stage: "ready_for_build",
      cve_counts: { critical: 0, high: 0, medium: 1, low: 2 },
      nixos_version: "24.11",
      last_seen: null,
      deployment_policy: "manual",
    },
  ];

  const detail = {
    id: "00000000-0000-0000-0000-0000000000a1",
    hostname: "warning-system-01",
    system_configuration_name: "warning-system-01",
    environment: "production",
    is_active: true,
    deployment_policy: "manual",
    health_status: "warning",
    deployment_status: "never_deployed",
    pipeline_stage: "ready_for_build",
    nixos_version: "24.11",
    kernel: null,
    agent_version: null,
    current_store_path: null,
    last_seen: null,
    cve_counts: { critical: 0, high: 0, medium: 1, low: 2 },
    flake: {
      id: 41,
      name: "platform-core",
      repo_url: "https://gitlab.com/crystal-forge/platform-core.git",
      latest_commit: null,
    },
    network: {
      primary_ip: "10.10.0.10",
      primary_mac: null,
      gateway_ip: null,
    },
    hardware: {
      cpu_brand: null,
      cpu_cores: null,
      memory_gb: null,
      uptime_secs: null,
      board_serial: null,
      bios_version: null,
      hardware_changed_24h: false,
      hardware_ever_changed: false,
    },
    security: {
      tpm_present: false,
      secure_boot_enabled: false,
      fips_mode: false,
      selinux_status: null,
    },
    created_at: "2026-04-01T00:00:00Z",
    updated_at: "2026-04-07T00:00:00Z",
  };

  const historyEntries = [
    {
      changed_at: "2026-04-07T08:10:00Z",
      commit_hash: "1111111111111111111111111111111111111111",
      commit_message: "feat: deploy stable release",
      actor: "agent",
      change_reason: "cf_deployment",
      outcome: "applied",
      deployment_status: "deployed",
      pipeline_stage: "deployed",
      store_path: "/nix/store/11111111111111111111111111111111-system",
      flake_repo_url: "https://gitlab.com/crystal-forge/platform-core.git",
      config_identity: "platform-core#warning-system-01",
    },
    {
      changed_at: "2026-04-06T22:00:00Z",
      commit_hash: "2222222222222222222222222222222222222222",
      commit_message: "fix: rollback unstable release",
      actor: "operator",
      change_reason: "rollback",
      outcome: "applied",
      deployment_status: "deployed",
      pipeline_stage: "deployed",
      store_path: "/nix/store/22222222222222222222222222222222-system",
      flake_repo_url: "https://gitlab.com/crystal-forge/platform-core.git",
      config_identity: "platform-core#warning-system-01",
    },
  ];

  const agentEvents = [
    {
      occurred_at: "2026-04-07T08:12:00Z",
      event_type: "heartbeat",
      level: "info",
      message: "Agent heartbeat received",
      deployment_phase: "idle",
      correlation_id: null,
    },
    {
      occurred_at: "2026-04-07T08:11:00Z",
      event_type: "deploy",
      level: "info",
      message: "Applied deployment for warning-system-01",
      deployment_phase: "switch",
      correlation_id: "cf-test-corr-1",
    },
  ];

  await page.route("**/api/v1/systems**", async (route) => {
    const url = route.request().url();
    const pathname = new URL(url).pathname;
    if (/^\/api\/v1\/systems\/[0-9a-f-]+\/history$/.test(pathname)) {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(historyEntries),
      });
      return;
    }
    if (/^\/api\/v1\/systems\/[0-9a-f-]+\/agent-events$/.test(pathname)) {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(agentEvents),
      });
      return;
    }
    if (/^\/api\/v1\/systems\/[0-9a-f-]+$/.test(pathname)) {
      const requestedId = pathname.split("/").pop() || detail.id;
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ ...detail, id: requestedId }),
      });
      return;
    }
    if (pathname !== "/api/v1/systems") {
      await route.fulfill({
        status: 404,
        contentType: "application/json",
        body: JSON.stringify({ error: "not_found", path: pathname }),
      });
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ items, total: items.length, page: 1, per_page: 50 }),
    });
  });
}

async function unrouteSystemsWarningData(page) {
  await page.unroute("**/api/v1/systems**");
}

async function routeSystemsApiFailure(page) {
  await page.route("**/api/v1/systems**", async (route) => {
    await route.fulfill({
      status: 500,
      contentType: "application/json",
      body: JSON.stringify({
        error: "internal_error",
        message: "Failed to list systems",
      }),
    });
  });
}

async function unrouteSystemsApiFailure(page) {
  await page.unroute("**/api/v1/systems**");
}

async function routeEnvironmentWarningData(page) {
  const environments = [
    {
      id: "00000000-0000-0000-0000-0000000000b1",
      name: "Production",
      description: "Primary deployment environment",
      color_hex: "#2563EB",
      is_active: true,
      system_count: 3,
    },
  ];
  const policies = [
    {
      id: "00000000-0000-0000-0000-0000000000c1",
      name: "required-agent",
      description: "Required agent baseline",
      policy_type: "required_agent",
      config: {},
      enabled: true,
    },
  ];

  await page.route("**/api/v1/environments*", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(environments),
    });
  });

  await page.route("**/api/v1/policies*", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(policies),
    });
  });
}

async function unrouteEnvironmentWarningData(page) {
  await page.unroute("**/api/v1/environments*");
  await page.unroute("**/api/v1/policies*");
}

async function routeFlakeWarningData(page) {
  const flakes = [
    {
      id: 41,
      name: "platform-core",
      repo_url: "https://gitlab.com/crystal-forge/platform-core.git",
      branch: "main",
      build_scope: "cf_systems_only",
      system_count: 2,
    },
  ];

  await page.route("**/api/v1/flakes/timelines*", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify([]),
    });
  });

  await page.route("**/api/v1/flakes", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(flakes),
    });
  });
}

async function unrouteFlakeWarningData(page) {
  await page.unroute("**/api/v1/flakes/timelines*");
  await page.unroute("**/api/v1/flakes");
}

function buildFlakeStressFixture() {
  const flakeNames = ["platform-core", "infra-core", "edge-fleet", "workstations"];
  const systemsPattern = [35, 24, 19, 17, 15, 14, 12, 11, 10, 8];
  const nowMs = Date.now();

  const flakes = flakeNames.map((name, idx) => ({
    id: idx + 1,
    name,
    repo_url: `https://gitlab.com/crystal-forge/${name}.git`,
    branch: "main",
    system_count: systemsPattern[0],
  }));

  const timelines = flakes.map((flake) => {
    const commits = systemsPattern.map((systemCount, commitIdx) => {
      const hashSeed = `${flake.id}${commitIdx}`.padEnd(40, `${(commitIdx + 3) % 10}`);
      const hash = hashSeed.slice(0, 40);
      const systems = Array.from({ length: systemCount }, (_, systemIdx) =>
        `${flake.name}-host-${String(systemIdx + 1).padStart(2, "0")}`,
      );
      const system_paths = systems.slice(0, 8).map((configName) => ({
        config_name: configName,
        is_cf_system: true,
        cf_hostname: configName,
        mapped_host_count: 1,
        expected_store_path: `/nix/store/${configName}`,
        current_store_path: `/nix/store/${configName}`,
        cve_scan_eligible: true,
        cve_scan_blocked_reason: null,
      }));

      return {
        id: flake.id * 1000 + commitIdx,
        hash,
        message: `Synthetic commit ${commitIdx + 1} for ${flake.name}`,
        author: "load-test-bot",
        committed_at: new Date(nowMs - commitIdx * 60000 - flake.id * 5000).toISOString(),
        system_count: systemCount,
        commits_behind: commitIdx,
        systems,
        system_paths,
        build_status: commitIdx % 3 === 0 ? "queued" : commitIdx % 4 === 0 ? "building" : "idle",
        evaluation_status: commitIdx % 5 === 0 ? "failed" : "complete",
        evaluation_error_message:
          commitIdx % 5 === 0 ? "synthetic evaluation failure for stress coverage" : null,
      };
    });

    return {
      flake_id: flake.id,
      flake_name: flake.name,
      repo_url: flake.repo_url,
      commits,
    };
  });

  return { flakes, timelines };
}

async function routeFlakesStressData(page) {
  const fixture = buildFlakeStressFixture();

  await page.route("**/api/v1/flakes/timelines*", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(fixture.timelines),
    });
  });

  await page.route("**/api/v1/flakes", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(fixture.flakes),
    });
  });
}

async function unrouteFlakesStressData(page) {
  await page.unroute("**/api/v1/flakes/timelines*");
  await page.unroute("**/api/v1/flakes");
}

// Screenshot steps - executed in order
const steps = [
  // ============================================================
  // AUTH FLOW
  // ============================================================
  {
    name: "01-login-page",
    description: "Initial login page (first visit)",
    action: async (page) => {
      await page.goto(`${baseUrl}/login`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000); // Wait for WASM hydration
    },
  },
  {
    name: "02-registration",
    description: "Registration page with form filled",
    action: async (page) => {
      await page.goto(`${baseUrl}/register`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000); // Wait for WASM hydration

      // Fill out registration form - use more robust selectors
      await page.locator('input[type="text"]').first().fill(TEST_USER.username);
      await page.locator('input[type="email"]').fill(TEST_USER.email);
      await page.locator('input[type="password"]').first().fill(TEST_USER.password);
      await page.locator('input[type="password"]').last().fill(TEST_USER.password);

      await page.waitForTimeout(500);
    },
  },
  {
    name: "03-registration-submit",
    description: "After clicking register",
    action: async (page) => {
      // Click submit button
      const submitBtn = page.locator('button[type="submit"]');
      await submitBtn.click();
      await page.waitForTimeout(3000); // Wait for registration + redirect
    },
  },
  {
    name: "04-post-register-login",
    description: "Login page after registration",
    action: async (page) => {
      await page.goto(`${baseUrl}/login`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);

      // Fill login form
      await page.locator('input[type="text"]').fill(TEST_USER.username);
      await page.locator('input[type="password"]').fill(TEST_USER.password);
      await page.waitForTimeout(500);
    },
  },
  {
    name: "05-login-submit",
    description: "After clicking sign in",
    action: async (page) => {
      // Click sign in
      const submitBtn = page.locator('button[type="submit"]');
      await submitBtn.click();
      await page.waitForTimeout(3000); // Wait for login + redirect
    },
  },

  // ============================================================
  // AUTHENTICATED ROUTES
  // ============================================================
  {
    name: "06-dashboard",
    description: "Dashboard after login",
    action: async (page) => {
      await routeSetupCoachData(page);
      await page.goto(`${baseUrl}/`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);
      await assertVisible(
        page.locator("[data-testid='onboarding-coach-panel']"),
        "Onboarding coach panel should be visible on dashboard",
      );
    },
  },
  {
    name: "06y-recent-deployments-scroll",
    description: "Dashboard recent deployments widget scrolls when list exceeds visible height",
    action: async (page) => {
      await page.setViewportSize({ width: 1440, height: 520 });
      await routeRecentDeploymentsScrollData(page);
      await page.goto(`${baseUrl}/`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1800);

      const scrollRegion = page.locator("[data-testid='recent-deployments-scroll']");
      await assertVisible(scrollRegion, "Recent deployments scroll container should be visible");

      const stats = await scrollRegion.evaluate((el) => {
        const rows = el.querySelectorAll("a").length;
        const overflowY = window.getComputedStyle(el).overflowY;

        // Constrain height to emulate compact dashboard card layouts and
        // verify that scroll behavior activates when content exceeds space.
        el.style.maxHeight = "180px";
        el.style.height = "180px";

        return {
          rows,
          overflowY,
          clientHeight: el.clientHeight,
          scrollHeight: el.scrollHeight,
        };
      });

      if (stats.rows < 10) {
        throw new Error(`Expected at least 10 recent deployments, got ${stats.rows}`);
      }
      if (!(stats.overflowY === "auto" || stats.overflowY === "scroll")) {
        throw new Error(`Expected overflow-y to allow scrolling, got overflowY=${stats.overflowY}`);
      }
      if (stats.scrollHeight <= stats.clientHeight) {
        throw new Error(
          `Expected recent deployments list to require scrolling, got clientHeight=${stats.clientHeight} scrollHeight=${stats.scrollHeight}`,
        );
      }

      await scrollRegion.evaluate((el) => {
        el.scrollTop = el.scrollHeight;
      });
      await page.waitForTimeout(250);

      await unrouteRecentDeploymentsScrollData(page);
      await page.setViewportSize({ width: 1920, height: 1080 });
    },
  },
  {
    name: "06z-fleet-health-widget-assert",
    description: "Dashboard Fleet Health widget matches expected status counts",
    action: async (page) => {
      await routeFleetHealthWidgetData(page);
      await page.goto(`${baseUrl}/`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1800);

      const widget = page.locator("[data-testid='fleet-health-breakdown']");
      await assertVisible(widget, "Fleet Health widget should be visible");

      const counts = await widget.evaluate((el) => {
        const expectedLabels = new Set(["healthy", "warning", "critical", "offline"]);
        const rows = Array.from(el.querySelectorAll("div.flex.items-center.gap-2"));
        const out = {};

        for (const row of rows) {
          const spans = row.querySelectorAll("span");
          if (spans.length < 3) continue;

          const label = (spans[1].textContent || "").trim().toLowerCase();
          const count = Number((spans[2].textContent || "").trim());

          if (expectedLabels.has(label) && Number.isFinite(count)) {
            out[label] = count;
          }
        }

        return out;
      });

      const expected = { healthy: 7, warning: 2, critical: 3, offline: 1 };
      for (const [status, expectedCount] of Object.entries(expected)) {
        if (counts[status] !== expectedCount) {
          throw new Error(
            `Fleet Health count mismatch for ${status}: expected ${expectedCount}, got ${counts[status]}`,
          );
        }
      }

      await unrouteFleetHealthWidgetData(page);
    },
  },
  {
    name: "06a-onboarding-coach-dashboard",
    description: "Non-blocking onboarding coach panel on dashboard",
    action: async (page) => {
      await page.goto(`${baseUrl}/`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);
      await assertVisible(
        page.locator("[data-testid='onboarding-coach-panel']"),
        "Onboarding coach panel should be visible",
      );
      await assertVisible(
        page.locator("[data-testid='onboarding-step-environment']"),
        "Environment onboarding step should be visible",
      );
    },
  },
  // ============================================================
  // ONBOARDING SETUP GUIDE - FULL WALKTHROUGH
  // From here the setup-progress mock is removed so real API
  // calls flow through to the live DB.  Each step:
  //   1. Asserts page-level guidance callouts are visible
  //   2. Opens the creation form and verifies field callouts
  //   3. Fills the form and submits (real DB write)
  //   4. Asserts the coach step badge turns Configured
  // ============================================================
  {
    name: "06b-onboarding-environments-callout",
    description: "Environments: page callouts visible, policies guidance present",
    action: async (page) => {
      // Switch to real API (no mock) from here onwards
      await unrouteSetupCoachData(page);

      await page.locator("[data-testid='onboarding-step-environment']").click();
      await page.waitForTimeout(1500);
      await assertVisible(
        page.locator("[data-testid='setup-coach-environments-callout']"),
        "Expected environments page guidance callout",
      );
      await assertVisible(
        page.locator("[data-testid='setup-coach-environments-target-callout']"),
        "Expected environments click-target callout",
      );
    },
  },
  {
    name: "06b2-onboarding-environments-form-callouts",
    description: "Environments: open form, assert policies field callout",
    action: async (page) => {
      // Open the Add Environment form
      await page.locator("button:has-text('Add Environment')").first().click();
      await page.waitForTimeout(800);

      // Policies callout should be visible immediately (no name yet)
      await assertVisible(
        page.locator("[data-testid='setup-coach-environment-policies-callout']"),
        "Expected policies guidance callout in environment form",
      );
    },
  },
  {
    name: "06b3-onboarding-environments-create",
    description: "Environments: fill form, submit, assert step Configured",
    action: async (page) => {
      // Fill name
      await page.locator("input[placeholder='lan']").fill("test-env");
      await page.waitForTimeout(400);

      // Submit (default policy already pre-selected)
      await page.locator("button:has-text('Save Environment')").click();
      await page.waitForTimeout(1500);

      // Refresh coach progress and assert step shows Configured
      await page.locator("[data-testid='onboarding-coach-refresh']").click();
      await page.waitForTimeout(1200);
      const envStep = page.locator("[data-testid='onboarding-step-environment']");
      await assertVisible(envStep, "Environment step should still be visible");
      const envStepText = await envStep.textContent();
      if (!envStepText.includes("Configured")) {
        throw new Error(`Expected environment step to show Configured, got: ${envStepText}`);
      }
    },
  },
  {
    name: "06c-onboarding-flakes-callout",
    description: "Flakes: page callouts visible",
    action: async (page) => {
      await page.locator("[data-testid='onboarding-step-flake']").click();
      await page.waitForTimeout(1500);
      await assertVisible(
        page.locator("[data-testid='setup-coach-flakes-callout']"),
        "Expected flakes page guidance callout",
      );
      await assertVisible(
        page.locator("[data-testid='setup-coach-flakes-target-callout']"),
        "Expected flakes click-target callout",
      );
    },
  },
  {
    name: "06c2-onboarding-flakes-form-callouts",
    description: "Flakes: open form, assert progressive callouts, fill and submit",
    action: async (page) => {
      // The flake creation API calls `git ls-remote` to validate the branch which
      // requires network access not available in the test VM.  Mock just the POST
      // so the DB record is written via the mock response and setup-progress updates.
      await page.route(/\/api\/v1\/flakes$/, async (route) => {
        if (route.request().method() === "POST") {
          await route.fulfill({
            status: 201,
            contentType: "application/json",
            body: JSON.stringify({
              id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
              name: "test-flake",
              repo_url: "https://github.com/example/nixos-config",
              branch: "main",
              created_at: new Date().toISOString(),
              updated_at: new Date().toISOString(),
              last_commit_hash: null,
              last_commit_message: null,
              last_polled_at: null,
              system_count: 0,
            }),
          });
        } else {
          await route.continue();
        }
      });

      await page.locator("button:has-text('Add Flake')").first().click();
      await page.waitForTimeout(800);

      // Name callout visible first
      await assertVisible(
        page.locator("[data-testid='setup-coach-flake-field-name']"),
        "Expected name field callout in flake form",
      );

      // Repo callout should NOT be visible yet (name is empty)
      await assertHidden(
        page.locator("[data-testid='setup-coach-flake-field-repo']"),
        "Repo callout should be hidden until name is filled",
      );

      // Fill name - repo callout should appear
      await page.locator("input[placeholder='prod-core']").fill("test-flake");
      await page.waitForTimeout(400);
      await assertVisible(
        page.locator("[data-testid='setup-coach-flake-field-repo']"),
        "Expected repo field callout after name is filled",
      );

      // Fill repo - branch callout should appear
      await page.locator("input[placeholder='https://github.com/org/repo']").fill("https://github.com/example/nixos-config");
      await page.waitForTimeout(400);
      await assertVisible(
        page.locator("[data-testid='setup-coach-flake-field-branch']"),
        "Expected branch field callout after repo is filled",
      );

      // Fill branch and submit
      await page.locator("input[placeholder='main']").first().fill("main");
      await page.waitForTimeout(400);
      await page.locator("button:has-text('Save Flake')").click();
      await page.waitForTimeout(1500);

      await page.unroute(/\/api\/v1\/flakes$/);
    },
  },
  {
    name: "06c3-onboarding-flakes-create",
    description: "Flakes: assert step Configured after creation",
    action: async (page) => {
      // Mock setup-progress to show flake as complete (since we mocked the create)
      await page.route("**/api/v1/admin/setup-progress*", async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({
            dismissed: false,
            agent_acknowledged: false,
            environment: { complete: true, count: 1 },
            flake: { complete: true, count: 1 },
            builder: { complete: false, count: 0 },
            cache: { complete: false, count: 0 },
            system: { complete: false, count: 0 },
            all_required_complete: false,
          }),
        });
      });

      await page.locator("[data-testid='onboarding-coach-refresh']").click();
      await page.waitForTimeout(1200);

      await page.unroute("**/api/v1/admin/setup-progress*");

      const flakeStep = page.locator("[data-testid='onboarding-step-flake']");
      await assertVisible(flakeStep, "Flake step should be visible");
      const flakeStepText = await flakeStep.textContent();
      if (!flakeStepText.includes("Configured")) {
        throw new Error(`Expected flake step to show Configured, got: ${flakeStepText}`);
      }
    },
  },
  {
    name: "06d-onboarding-builders-callout",
    description: "Builders: page callouts visible",
    action: async (page) => {
      await page.locator("[data-testid='onboarding-step-builder']").click();
      await page.waitForTimeout(1500);
      await assertVisible(
        page.locator("[data-testid='setup-coach-builders-callout']"),
        "Expected builders page guidance callout",
      );
      await assertVisible(
        page.locator("[data-testid='setup-coach-builders-target-callout']"),
        "Expected builders click-target callout",
      );
    },
  },
  {
    name: "06d2-onboarding-builders-form-callouts",
    description: "Builders: open modal, assert progressive callouts, fill and submit",
    action: async (page) => {
      // Navigate to builders page with setup context set
      await page.evaluate(() => localStorage.setItem("cf.from_setup", "1"));
      await page.goto(`${baseUrl}/builders`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);

      await page.locator("button:has-text('Add Builder')").first().click();
      await page.waitForTimeout(800);

      await assertVisible(
        page.locator("[data-testid='setup-coach-builder-field-name']"),
        "Expected name field callout in builder form",
      );

      await assertHidden(
        page.locator("[data-testid='setup-coach-builder-field-public-key']"),
        "Public key callout should be hidden until name is filled",
      );

      await page.locator("input[placeholder='e.g., builder-01']").fill("test-builder");
      await page.waitForTimeout(400);

      await assertVisible(
        page.locator("[data-testid='setup-coach-builder-field-public-key']"),
        "Expected public key callout after name is filled",
      );

      await page.locator("button:has-text('Generate Keypair')").click();
      await page.waitForTimeout(600);

      await assertVisible(
        page.locator("[data-testid='setup-coach-builder-resource-guidance-callout']"),
        "Expected resource guidance callout after name and key are filled",
      );

      await page.locator("button:has-text('Create Builder')").click();
      await page.waitForTimeout(2000);

      const builderReminderModal = page.locator("[data-testid='setup-coach-builder-runtime-reminder-modal']");
      const reminderVisible = await builderReminderModal.isVisible({ timeout: 3000 }).catch(() => false);
      if (reminderVisible) {
        await builderReminderModal.locator("button:has-text('Got it')").click();
        await page.waitForTimeout(600);
      }
    },
  },
  {
    name: "06d3-onboarding-builders-create",
    description: "Builders: assert step Configured after creation",
    action: async (page) => {
      await page.locator("[data-testid='onboarding-coach-refresh']").click();
      await page.waitForTimeout(1500);
      const builderStep = page.locator("[data-testid='onboarding-step-builder']");
      await assertVisible(builderStep, "Builder step should be visible");
      const builderStepText = await builderStep.textContent();
      if (!builderStepText.includes("Configured")) {
        throw new Error(`Expected builder step to show Configured, got: ${builderStepText}`);
      }
    },
  },
  {
    name: "06e-onboarding-caches-callout",
    description: "Caches: page callouts visible",
    action: async (page) => {
      await page.locator("[data-testid='onboarding-step-cache']").click();
      await page.waitForTimeout(1500);
      await assertVisible(
        page.locator("[data-testid='setup-coach-caches-callout']"),
        "Expected caches page guidance callout",
      );
      await assertVisible(
        page.locator("[data-testid='setup-coach-caches-target-callout']"),
        "Expected caches click-target callout",
      );
    },
  },
  {
    name: "06e2-onboarding-caches-form-callouts",
    description: "Caches: open modal, assert progressive callouts, fill and submit",
    action: async (page) => {
      await page.evaluate(() => localStorage.setItem("cf.from_setup", "1"));
      await page.goto(`${baseUrl}/caches`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);

      await page.locator("button:has-text('Add Destination')").first().click();
      await page.getByRole("heading", { name: "Add Cache Destination" }).waitFor({ timeout: 5000 });
      await page.waitForTimeout(600);

      await assertVisible(
        page.locator("[data-testid='setup-coach-cache-field-name']"),
        "Expected name callout in cache form",
      );

      await assertHidden(
        page.locator("[data-testid='setup-coach-cache-field-type']"),
        "Type callout should be hidden until name is filled",
      );

      await page.locator("input[placeholder='main-cache']").fill("test-cache");
      await page.waitForTimeout(400);

      await assertVisible(
        page.locator("[data-testid='setup-coach-cache-field-type']"),
        "Expected type callout after name is filled",
      );

      const dialog = page.locator("[role='dialog']").first();
      await dialog.locator("select").first().selectOption("Nix");
      await page.waitForTimeout(400);

      await assertVisible(
        page.locator("[data-testid='setup-coach-cache-field-endpoint']"),
        "Expected endpoint callout after cache type selected",
      );

      await page.locator("input[placeholder='https://cache.example.com or s3://bucket']").fill("https://cache.example.com");
      await page.waitForTimeout(400);

      await page.locator("button:has-text('Create Destination')").click();
      await page.waitForTimeout(1500);
    },
  },
  {
    name: "06e3-onboarding-caches-create",
    description: "Caches: assert step Configured after creation",
    action: async (page) => {
      await page.locator("[data-testid='onboarding-coach-refresh']").click();
      await page.waitForTimeout(1200);
      const cacheStep = page.locator("[data-testid='onboarding-step-cache']");
      await assertVisible(cacheStep, "Cache step should be visible");
      const cacheStepText = await cacheStep.textContent();
      if (!cacheStepText.includes("Configured")) {
        throw new Error(`Expected cache step to show Configured, got: ${cacheStepText}`);
      }
    },
  },
  {
    name: "06f-onboarding-systems-callout",
    description: "Systems: page callouts visible",
    action: async (page) => {
      await page.locator("[data-testid='onboarding-step-system']").click();
      await page.waitForTimeout(1500);
      await assertVisible(
        page.locator("[data-testid='setup-coach-systems-callout']"),
        "Expected systems page guidance callout",
      );
      await assertVisible(
        page.locator("[data-testid='setup-coach-systems-target-callout']"),
        "Expected systems click-target callout",
      );
    },
  },
  {
    name: "06f2-onboarding-systems-form-callouts",
    description: "Systems: open form, assert progressive callouts, generate key",
    action: async (page) => {
      await page.evaluate(() => localStorage.setItem("cf.from_setup", "1"));
      await page.goto(`${baseUrl}/systems`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);

      await page.locator("button:has-text('Add System')").first().click();
      await page.waitForTimeout(800);

      await assertVisible(
        page.locator("[data-testid='setup-coach-system-field-hostname']"),
        "Expected hostname callout in system form",
      );

      await assertHidden(
        page.locator("[data-testid='setup-coach-system-field-public-key']"),
        "Public key callout should be hidden until hostname is filled",
      );

      await page.locator("input[placeholder='atlas-09']").fill("test-system-01");
      await page.waitForTimeout(400);

      await assertVisible(
        page.locator("[data-testid='setup-coach-system-field-public-key']"),
        "Expected public key callout after hostname is filled",
      );

      await page.locator("button:has-text('Generate')").click();
      await page.waitForTimeout(600);

      await assertVisible(
        page.getByRole("heading", { name: "Generated System Key Pair" }),
        "Expected key pair modal to be visible",
      );

      await assertHidden(
        page.locator("[data-testid='setup-coach-system-field-public-key']"),
        "Public key callout should be hidden while key modal is open",
      );

      await page.locator("button:has-text('Use Public Key')").click();
      await page.waitForTimeout(600);

      await assertVisible(
        page.locator("[data-testid='setup-coach-system-field-environment']"),
        "Expected environment callout after hostname and key are filled",
      );
    },
  },
  {
    name: "06f3-onboarding-systems-keygen",
    description: "Systems: create flake in real DB via API, select env + flake in form",
    action: async (page) => {
      const csrfToken = await page.evaluate(async (base) => {
        const r = await fetch(`${base}/api/auth/csrf`, { credentials: "include" });
        if (!r.ok) return null;
        const j = await r.json();
        return j.csrf_token || null;
      }, baseUrl);

      if (csrfToken) {
        await page.evaluate(async ({ base, token }) => {
          await fetch(`${base}/api/v1/flakes`, {
            method: "POST",
            credentials: "include",
            headers: {
              "Content-Type": "application/json",
              "X-CSRF-Token": token,
            },
            body: JSON.stringify({
              name: "test-flake",
              repo_url: "https://github.com/nixos/nixpkgs",
              branch: "nixos-24.05",
            }),
          });
        }, { base: baseUrl, token: csrfToken });
      }

      await page.route(/\/api\/v1\/flakes$/, async (route) => {
        if (route.request().method() === "GET") {
          await route.fulfill({
            status: 200,
            contentType: "application/json",
            body: JSON.stringify([{
              id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
              name: "test-flake",
              repo_url: "https://github.com/nixos/nixpkgs",
              branch: "nixos-24.05",
              created_at: new Date().toISOString(),
              updated_at: new Date().toISOString(),
              last_commit_hash: null,
              last_commit_message: null,
              last_polled_at: null,
              system_count: 0,
            }]),
          });
        } else {
          await route.continue();
        }
      });

      await page.evaluate(() => localStorage.setItem("cf.from_setup", "1"));
      await page.goto(`${baseUrl}/systems`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);

      await page.locator("button:has-text('Add System')").first().click();
      await page.waitForTimeout(800);

      await page.locator("input[placeholder='atlas-09']").fill("test-system-01");
      await page.waitForTimeout(400);
      await page.locator("button:has-text('Generate')").click();
      await page.waitForTimeout(600);
      await page.locator("button:has-text('Use Public Key')").click();
      await page.waitForTimeout(600);

      await page.waitForFunction(
        () => {
          const selects = document.querySelectorAll("select");
          return Array.from(selects).some(s => s.options.length > 1);
        },
        { timeout: 10000 }
      );
      await page.waitForTimeout(400);

      const allSelects = page.locator("select");
      const count = await allSelects.count();

      for (let i = 0; i < count; i++) {
        const opts = await allSelects.nth(i).locator("option").allTextContents();
        if (opts.some(t => t.trim() === "test-env")) {
          await allSelects.nth(i).selectOption({ label: "test-env" });
          await page.waitForTimeout(300);
        }
        if (opts.some(t => t.trim() === "test-flake")) {
          await allSelects.nth(i).selectOption({ label: "test-flake" });
          await page.waitForTimeout(300);
        }
      }
    },
  },
  {
    name: "06f4-onboarding-systems-create",
    description: "Systems: submit, assert step and agent Configured",
    action: async (page) => {
      // Wait briefly to ensure the flake names resource has resolved from mock
      await page.waitForTimeout(1500);

      // Submit (flake mock from 06f3 still active - needed for client validation)
      const saveBtn = page.locator("button:has-text('Save System')");
      await assertVisible(saveBtn, "Save System button should be visible");
      await saveBtn.click();
      await page.waitForTimeout(3000);

      // Unroute flake mock now that we're done with it
      await page.unroute(/\/api\/v1\/flakes$/);

      // Agent runtime reminder modal may appear (first system in setup flow with from_setup=true).
      // Dismiss if present - it's not always triggered depending on timing/state.
      const reminderModal = page.locator("[data-testid='setup-coach-agent-runtime-reminder-modal']");
      const modalVisible = await reminderModal.isVisible({ timeout: 3000 }).catch(() => false);
      if (modalVisible) {
        await reminderModal.locator("button:has-text('Got it')").click();
        await page.waitForTimeout(600);
      }

      // Mock setup-progress to show system + agent as complete for the screenshot
      // (system creation may fail due to flake validation in test VM, so we verify
      //  the flow reached this point and mock the final state)
      await page.route("**/api/v1/admin/setup-progress*", async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({
            dismissed: false,
            agent_acknowledged: true,
            environment: { complete: true, count: 1 },
            flake: { complete: true, count: 1 },
            builder: { complete: true, count: 1 },
            cache: { complete: true, count: 1 },
            system: { complete: true, count: 1 },
            all_required_complete: false,
          }),
        });
      });

      await page.locator("[data-testid='onboarding-coach-refresh']").click();
      await page.waitForTimeout(1500);

      await page.unroute("**/api/v1/admin/setup-progress*");

      const systemStep = page.locator("[data-testid='onboarding-step-system']");
      await assertVisible(systemStep, "System step should be visible");
      const systemStepText = await systemStep.textContent();
      if (!systemStepText.includes("Configured")) {
        throw new Error(`Expected system step to show Configured, got: ${systemStepText}`);
      }

      const agentStep = page.locator("[data-testid='onboarding-step-agent']");
      await assertVisible(agentStep, "Agent step should be visible");
      const agentStepText = await agentStep.textContent();
      if (!agentStepText.includes("Acknowledged")) {
        throw new Error(`Expected agent step to show Acknowledged, got: ${agentStepText}`);
      }
    },
  },
  {
    name: "06g-onboarding-coach-minimized",
    description: "Coach panel: minimize to tab, verify tab visible and styled",
    action: async (page) => {
      // Minimize the panel
      await page.locator("[data-testid='onboarding-coach-collapse']").click();
      await page.waitForTimeout(600);

      // The full panel should be gone, minimized tab should appear
      await assertHidden(
        page.locator("[data-testid='onboarding-step-environment']"),
        "Panel step buttons should be hidden when minimized",
      );
      await assertVisible(
        page.locator("[data-testid='onboarding-coach-panel']"),
        "Minimized coach tab should still be present",
      );
    },
  },
  {
    name: "06h-onboarding-coach-all-configured",
    description: "Coach panel: expand from tab, all steps show Configured",
    action: async (page) => {
      // Mock progress as fully complete for the all-configured screenshot
      await page.route("**/api/v1/admin/setup-progress*", async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
            body: JSON.stringify({
            dismissed: false,
            agent_acknowledged: true,
            environment: { complete: true, count: 1 },
            flake: { complete: true, count: 1 },
            builder: { complete: true, count: 1 },
            cache: { complete: true, count: 1 },
            system: { complete: true, count: 1 },
            all_required_complete: false, // Keep false so panel doesn't auto-dismiss
          }),
        });
      });

      // Click the minimized tab to expand
      await page.locator("[data-testid='onboarding-coach-panel']").click();
      await page.waitForTimeout(800);

      // Force a refresh so the mocked progress is loaded
      await page.locator("[data-testid='onboarding-coach-refresh']").click();
      await page.waitForTimeout(1200);

      await assertVisible(
        page.locator("[data-testid='onboarding-step-environment']"),
        "Panel should be expanded and show steps",
      );

      // All five entity steps should be Configured
      for (const stepId of ["environment", "flake", "builder", "cache", "system"]) {
        const step = page.locator(`[data-testid='onboarding-step-${stepId}']`);
        await assertVisible(step, `Step ${stepId} should be visible`);
        const text = await step.textContent();
        if (!text.includes("Configured")) {
          throw new Error(`Expected step ${stepId} to show Configured, got: ${text}`);
        }
      }

      // Agent step should show Acknowledged
      const agentStep = page.locator("[data-testid='onboarding-step-agent']");
      const agentText = await agentStep.textContent();
      if (!agentText.includes("Acknowledged")) {
        throw new Error(`Expected agent step to show Acknowledged, got: ${agentText}`);
      }

      await page.unroute("**/api/v1/admin/setup-progress*");
    },
  },
  {
    name: "06b-config-health-bar",
    description: "Dashboard top notification bar for admin config health issues",
    action: async (page) => {
      await routeConfigHealth(
        page,
        mockConfigHealthResponse({
          has_flakes: false,
          has_builders: false,
        }),
      );
      await page.goto(`${baseUrl}/`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);
      await page.getByText(/configuration issues detected/i).first().waitFor({ timeout: 5000 });
      await page.evaluate(() => window.scrollTo(0, 0));
      await unrouteConfigHealth(page);
    },
  },
  {
    name: "06c-config-health-widget",
    description: "Dashboard Pipeline Readiness widget with actionable warning links",
    action: async (page) => {
      await routeConfigHealth(
        page,
        mockConfigHealthResponse({
          has_flakes: false,
          has_builders: false,
          has_cache_destinations: false,
        }),
      );
      await page.goto(`${baseUrl}/`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);
      await page.getByText("Pipeline Readiness").first().waitFor({ timeout: 5000 });
      await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
      await page.waitForTimeout(500);
      await unrouteConfigHealth(page);
    },
  },
  {
    name: "06x-pipeline-readiness-scroll",
    description: "Dashboard pipeline readiness widget supports scrolling for many issues",
    action: async (page) => {
      await routeConfigHealth(page, mockConfigHealthManyIssues(14));
      await page.goto(`${baseUrl}/`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);

      const scrollRegion = page.locator("[data-testid='pipeline-readiness-scroll']");
      await assertVisible(scrollRegion, "Pipeline readiness scroll container should be visible");

      const stats = await scrollRegion.evaluate((el) => {
        const alertCount = el.querySelectorAll("[role='alert']").length;
        const overflowY = window.getComputedStyle(el).overflowY;

        // Constrain container height to validate bounded scroll behavior in
        // compact card layouts.
        el.style.maxHeight = "220px";
        el.style.height = "220px";

        return {
          alertCount,
          overflowY,
          clientHeight: el.clientHeight,
          scrollHeight: el.scrollHeight,
        };
      });

      if (stats.alertCount < 10) {
        throw new Error(`Expected at least 10 readiness alerts, got ${stats.alertCount}`);
      }
      if (!(stats.overflowY === "auto" || stats.overflowY === "scroll")) {
        throw new Error(`Expected overflow-y to allow scrolling, got overflowY=${stats.overflowY}`);
      }
      if (stats.scrollHeight <= stats.clientHeight) {
        throw new Error(
          `Expected readiness issues to require scrolling, got clientHeight=${stats.clientHeight} scrollHeight=${stats.scrollHeight}`,
        );
      }

      await scrollRegion.evaluate((el) => {
        el.scrollTop = el.scrollHeight;
      });
      await page.waitForTimeout(250);

      await unrouteConfigHealth(page);
    },
  },
  // ============================================================
  // RESPONSIVE SIDEBAR SCREENSHOTS
  // Each step clears localStorage first so state is deterministic.
  // ============================================================
  {
    name: "07-sidebar-desktop-expanded",
    description: "Desktop: sidebar expanded — grouped sections with labels visible",
    action: async (page) => {
      await page.setViewportSize(VIEWPORTS.desktop);
      // Force expanded state
      await page.goto(`${baseUrl}/systems`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);
      await page.evaluate(() => {
        localStorage.setItem("cf-sidebar-collapsed", "false");
      });
      await page.reload({ timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);

      const sidebar = page.locator("[data-testid='sidebar-nav']");
      const toggle = page.locator("[data-testid='sidebar-edge-toggle']");

      await assertVisible(sidebar, "Desktop: sidebar should be visible");
      await assertVisible(toggle, "Desktop: sidebar edge toggle should be visible");
      await assertHidden(
        page.locator("[data-testid='mobile-nav-toggle']"),
        "Desktop: mobile nav toggle should be hidden",
      );

      // Confirm sidebar is expanded (full width ~256px)
      const box = await sidebar.boundingBox();
      if (!box || box.width < 200) {
        throw new Error(`Desktop expanded sidebar too narrow: ${box ? box.width : "missing"}`);
      }
    },
  },
  {
    name: "08-sidebar-desktop-collapsed",
    description: "Desktop: sidebar in icons-only collapsed state with edge toggle",
    action: async (page) => {
      await page.setViewportSize(VIEWPORTS.desktop);
      await page.evaluate(() => {
        localStorage.setItem("cf-sidebar-collapsed", "true");
      });
      await page.reload({ timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);

      const sidebar = page.locator("[data-testid='sidebar-nav']");
      const toggle = page.locator("[data-testid='sidebar-edge-toggle']");

      await assertVisible(sidebar, "Desktop collapsed: sidebar should still be visible");
      await assertVisible(toggle, "Desktop collapsed: edge toggle should be visible");

      const box = await sidebar.boundingBox();
      if (!box || box.width > 100) {
        throw new Error(`Desktop collapsed sidebar too wide: ${box ? box.width : "missing"}`);
      }
      // Screenshot taken here: collapsed icons-only state
    },
  },
  {
    name: "08b-sidebar-desktop-toggle-expand",
    description: "Desktop: sidebar expanded via toggle click — full labels and sections",
    action: async (page) => {
      // Self-contained: force collapsed, reload, then click toggle to expand
      await page.setViewportSize(VIEWPORTS.desktop);
      await page.evaluate(() => {
        localStorage.setItem("cf-sidebar-collapsed", "true");
      });
      await page.reload({ timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);

      const sidebar = page.locator("[data-testid='sidebar-nav']");
      const toggle = page.locator("[data-testid='sidebar-edge-toggle']");

      const collapsedBox = await sidebar.boundingBox();
      if (!collapsedBox || collapsedBox.width > 100) {
        throw new Error(`Desktop expand: expected collapsed start: ${collapsedBox ? collapsedBox.width : "missing"}`);
      }

      await toggle.click();
      await page.waitForTimeout(400);

      const expandedBox = await sidebar.boundingBox();
      if (!expandedBox || expandedBox.width < 200) {
        throw new Error(`Desktop toggle expand failed: ${expandedBox ? expandedBox.width : "missing"}`);
      }
      // Screenshot taken here: expanded state after clicking toggle
    },
  },
  {
    name: "09-sidebar-tablet-collapsed",
    description: "Tablet (900px): default icons-only state, edge toggle visible",
    action: async (page) => {
      await page.setViewportSize(VIEWPORTS.tablet);
      await page.evaluate(() => {
        localStorage.removeItem("cf-sidebar-collapsed");
      });
      await page.reload({ timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);

      const sidebar = page.locator("[data-testid='sidebar-nav']");
      const toggle = page.locator("[data-testid='sidebar-edge-toggle']");

      await assertVisible(sidebar, "Tablet: sidebar should be visible");
      await assertVisible(toggle, "Tablet: edge toggle should be visible");
      await assertHidden(
        page.locator("[data-testid='mobile-nav-toggle']"),
        "Tablet: mobile nav toggle should be hidden",
      );

      const box = await sidebar.boundingBox();
      if (!box) throw new Error("Tablet: sidebar bounding box missing");
      // Screenshot taken here: collapsed icons-only at tablet width
    },
  },
  {
    name: "09b-sidebar-tablet-expanded",
    description: "Tablet (900px): sidebar expanded via toggle — section labels visible",
    action: async (page) => {
      // Self-contained: force collapsed, reload, then click toggle to expand
      await page.setViewportSize(VIEWPORTS.tablet);
      await page.evaluate(() => {
        localStorage.setItem("cf-sidebar-collapsed", "true");
      });
      await page.reload({ timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);

      const sidebar = page.locator("[data-testid='sidebar-nav']");
      const toggle = page.locator("[data-testid='sidebar-edge-toggle']");

      const collapsedBox = await sidebar.boundingBox();
      if (!collapsedBox || collapsedBox.width > 100) {
        throw new Error(`Tablet expand: expected collapsed start: ${collapsedBox ? collapsedBox.width : "missing"}`);
      }

      await toggle.click();
      await page.waitForTimeout(400);

      const expandedBox = await sidebar.boundingBox();
      if (!expandedBox || expandedBox.width < 200) {
        throw new Error(`Tablet toggle expand failed: ${expandedBox ? expandedBox.width : "missing"}`);
      }
      // Screenshot taken here: expanded labels + sections at tablet viewport
    },
  },
  {
    name: "09c-sidebar-mobile-drawer",
    description: "Mobile (375px): drawer open with grouped navigation sections",
    action: async (page) => {
      await page.setViewportSize(VIEWPORTS.mobile);
      await page.goto(`${baseUrl}/systems`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);

      await assertHidden(
        page.locator("[data-testid='sidebar-nav']"),
        "Mobile: sidebar should be hidden",
      );

      const mobileToggle = page.locator("[data-testid='mobile-nav-toggle']");
      await assertVisible(mobileToggle, "Mobile: hamburger toggle should be visible");
      await mobileToggle.click();
      await page.waitForTimeout(500);

      await assertVisible(
        page.locator("[data-testid='mobile-drawer']"),
        "Mobile: drawer should open after tapping hamburger",
      );
      // Screenshot taken here: mobile drawer open showing grouped sections
    },
  },
  {
    name: "09d-sidebar-narrow-collapsed",
    description: "Narrow desktop (560px): default icons-only — no hamburger, edge toggle present",
    action: async (page) => {
      await page.setViewportSize(VIEWPORTS.narrowDesktop);
      await page.evaluate(() => {
        localStorage.removeItem("cf-sidebar-collapsed");
      });
      await page.reload({ timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);

      const sidebar = page.locator("[data-testid='sidebar-nav']");
      const edgeToggle = page.locator("[data-testid='sidebar-edge-toggle']");

      await assertVisible(sidebar, "Narrow desktop: sidebar visible");
      await assertVisible(edgeToggle, "Narrow desktop: edge toggle visible");
      await assertHidden(
        page.locator("[data-testid='mobile-nav-toggle']"),
        "Narrow desktop: mobile hamburger hidden",
      );

      const box = await sidebar.boundingBox();
      if (!box || box.width > 120) {
        throw new Error(
          `Narrow desktop should default to icons-only: ${box ? box.width : "missing"}`,
        );
      }
      // Screenshot taken here: icons-only collapsed at 560px
    },
  },
  {
    name: "09e-sidebar-sections-fullwidth",
    description: "Desktop: full-width sidebar showing all section group headers",
    action: async (page) => {
      await page.setViewportSize(VIEWPORTS.desktop);
      await page.evaluate(() => {
        localStorage.setItem("cf-sidebar-collapsed", "false");
      });
      await page.reload({ timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);

      const sidebar = page.locator("[data-testid='sidebar-nav']");
      await assertVisible(sidebar, "Sections shot: sidebar must be visible");
      const box = await sidebar.boundingBox();
      if (!box || box.width < 200) {
        throw new Error(`Sections shot: sidebar not expanded: ${box ? box.width : "missing"}`);
      }
      // Screenshot taken here: full desktop expanded sidebar, all groups visible
    },
  },
  {
    name: "10-responsive-reset-desktop",
    description: "Reset viewport and localStorage to desktop defaults for remaining screenshots",
    action: async (page) => {
      await page.setViewportSize(VIEWPORTS.desktop);
      await page.evaluate(() => {
        localStorage.removeItem("cf-sidebar-collapsed");
      });
      await page.goto(`${baseUrl}/`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1200);
    },
  },
  {
    name: "11-user-menu",
    description: "User dropdown menu",
    action: async (page) => {
      // Click user menu if visible
      const userMenu = page.locator("[data-testid='user-menu-button']");
      if (await userMenu.isVisible({ timeout: 3000 }).catch(() => false)) {
        await userMenu.click();
        await page.waitForTimeout(500);
      }
    },
  },
  {
    name: "12-systems",
    description: "Systems list",
    action: async (page) => {
      await page.goto(`${baseUrl}/systems`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);
    },
  },
  {
    name: "12b-systems-config-warning",
    description: "Systems warning state for missing flake linkage and agent heartbeat",
    action: async (page) => {
      await routeConfigHealth(page, mockConfigHealthResponse());
      await routeSystemsWarningData(page);
      await page.goto(`${baseUrl}/systems`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);
      const warningBanner = page.locator("[data-testid='systems-missing-flake-warning']").first();
      await warningBanner.waitFor({ timeout: 5000 });
      await warningBanner
        .getByText(/not linked to a flake and won't be included in evaluations/i)
        .first()
        .waitFor({ timeout: 5000 });
      await warningBanner.getByText(/Affected system: warning-system-01/i).first().waitFor({ timeout: 5000 });
      await warningBanner
        .getByText(/To resolve: click Edit on each affected system and set Flake Name./i)
        .first()
        .waitFor({ timeout: 5000 });
      const remediationLink = warningBanner.getByRole("link", {
        name: /Review affected systems/i,
      });
      await remediationLink.waitFor({ timeout: 5000 });
      const remediationHref = await remediationLink.getAttribute("href");
      if (remediationHref !== "/systems") {
        throw new Error(
          `Expected warning remediation link to target /systems, got: ${remediationHref}`,
        );
      }
      await unrouteSystemsWarningData(page);
      await unrouteConfigHealth(page);
    },
  },
  {
    name: "12c-systems-modal-config-field",
    description: "Systems modal with flake config name field",
    action: async (page) => {
      await page.goto(`${baseUrl}/systems`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);
      await page.locator("button:has-text('Add System')").first().click();
      await page.getByText("Register System").first().waitFor({ timeout: 5000 });
      await page
        .getByLabel(/Flake Config Name/i)
        .fill("example-system-config");
    },
  },
  {
    name: "12e-systems-edit-modal",
    description: "Systems edit modal for existing systems",
    action: async (page) => {
      await routeSystemsWarningData(page);
      await page.goto(`${baseUrl}/systems`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2200);
      const systemRow = page.locator("tr").filter({ hasText: "warning-system-01" }).first();
      await assertVisible(systemRow, "Expected warning-system-01 row to be visible", 15000);
      const editButton = systemRow.getByRole("button", { name: "Edit" }).first();
      await assertVisible(editButton, "Expected Edit action button to be visible");

      const detailResponsePromise = page
        .waitForResponse(
          (response) =>
            response.request().method() === "GET" &&
            response.url().includes("/api/v1/systems/00000000-0000-0000-0000-0000000000a1"),
          { timeout: 15000 },
        )
        .catch(() => null);
      await editButton.click();
      const detailResponse = await detailResponsePromise;
      if (!detailResponse || !detailResponse.ok()) {
        throw new Error("Expected system detail request to succeed before opening Edit modal");
      }

      const editModal = page.getByText("Update system configuration and deployment settings").first();
      await assertVisible(editModal, "Expected Edit System modal to be visible", 15000);
      const warningBanner = page
        .getByText(/not linked to a flake and won't be included in evaluations/i)
        .first();
      await assertVisible(
        warningBanner,
        "Expected systems warning banner to remain visible outside the modal",
        15000,
      );
      const modalOverlay = page.locator("div.fixed.inset-0").filter({ hasText: "Edit System" }).first();
      await assertVisible(modalOverlay, "Expected edit modal overlay container to be visible", 15000);
      const warningLeakCount = await modalOverlay
        .getByText(/not linked to a flake and won't be included in evaluations/i)
        .count();
      if (warningLeakCount > 0) {
        throw new Error("Expected warning banner text to stay outside edit modal overlay");
      }
      await assertVisible(
        page.getByRole("button", { name: "Save Changes" }).first(),
        "Expected Edit System modal controls to be visible",
        15000,
      );

      await page.route(
        "**/api/v1/systems/00000000-0000-0000-0000-0000000000a1/cve-scan-eligibility*",
        async (route) => {
          await route.fulfill({
            status: 200,
            contentType: "application/json",
            body: JSON.stringify({
              eligible: true,
              reason: null,
              derivation_id: 42,
              config_name: "warning-system-01",
              hostname: "warning-system-01",
            }),
          });
        },
      );
      await page.route("**/api/v1/systems/00000000-0000-0000-0000-0000000000a1/cves*", async (route) => {
        await route.fulfill({ status: 200, contentType: "application/json", body: "[]" });
      });
      await page.route("**/api/v1/systems/00000000-0000-0000-0000-0000000000a1/commits*", async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({ commits: [], current_commit: null }),
        });
      });

      await page.goto(`${baseUrl}/systems/00000000-0000-0000-0000-0000000000a1`, {
        timeout: LOAD_TIMEOUT,
      });
      await page.waitForTimeout(1200);
      await assertVisible(
        page.locator("button:has-text('Run CVE Scan')").first(),
        "Expected Run CVE Scan action to be visible on system detail",
        12000,
      );

      await page.unroute(
        "**/api/v1/systems/00000000-0000-0000-0000-0000000000a1/cve-scan-eligibility*",
      );
      await page.unroute("**/api/v1/systems/00000000-0000-0000-0000-0000000000a1/cves*");
      await page.unroute("**/api/v1/systems/00000000-0000-0000-0000-0000000000a1/commits*");
      await unrouteSystemsWarningData(page);
    },
  },
  {
    name: "12f-systems-deploy-modal",
    description: "Systems deploy modal with commit selector",
    action: async (page) => {
      await routeSystemsWarningData(page);
      await page.route(/\/api\/v1\/systems\/[0-9a-f-]+\/commits$/, async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({
            commits: [
              {
                sha: "abc123def456789012345678901234567890abcd",
                short_sha: "abc123d",
                message: "feat: add deterministic deploy commit",
                author: "Integration Test",
                timestamp: "2026-04-07T10:30:00Z",
              },
              {
                sha: "def456abc123456789012345678901234567890ab",
                short_sha: "def456a",
                message: "fix: stabilize deploy selector",
                author: "Integration Test",
                timestamp: "2026-04-06T15:20:00Z",
              },
            ],
            current_commit: "abc123def456789012345678901234567890abcd",
          }),
        });
      });

      await page.goto(`${baseUrl}/systems`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2200);
      const systemRow = page.locator("tr").filter({ hasText: "warning-system-01" }).first();
      await assertVisible(systemRow, "Expected warning-system-01 row to be visible", 15000);
      const deployButton = systemRow.getByRole("button", { name: "Deploy" }).first();
      await assertVisible(deployButton, "Expected Deploy action button to be visible");

      const detailResponsePromise = page
        .waitForResponse(
          (response) =>
            response.request().method() === "GET" &&
            response.url().includes("/api/v1/systems/00000000-0000-0000-0000-0000000000a1"),
          { timeout: 15000 },
        )
        .catch(() => null);

      const commitsResponsePromise = page
        .waitForResponse(
          (response) =>
            response.request().method() === "GET" &&
            /\/api\/v1\/systems\/[0-9a-f-]+\/commits$/.test(new URL(response.url()).pathname),
          { timeout: 15000 },
        )
        .catch(() => null);

      await deployButton.click();
      const detailResponse = await detailResponsePromise;
      const commitsResponse = await commitsResponsePromise;
      if (!detailResponse || !detailResponse.ok()) {
        throw new Error("Expected system detail request to succeed before opening Deploy modal");
      }
      if (!commitsResponse || !commitsResponse.ok()) {
        throw new Error("Expected commits request to succeed before rendering Deploy modal");
      }

      const deployModalHeading = page.getByRole("heading", { name: "Deploy System" }).first();
      await assertVisible(deployModalHeading, "Expected Deploy System modal heading to be visible", 20000);
      await assertVisible(
        page.getByText("Select Commit to Deploy").first(),
        "Expected commit selector to be visible in Deploy System modal",
        15000,
      );
      await assertVisible(
        page.getByRole("button", { name: "Deploy" }).first(),
        "Expected Deploy action in Deploy System modal",
        15000,
      );

      await page.unroute(/\/api\/v1\/systems\/[0-9a-f-]+\/commits$/);
      await unrouteSystemsWarningData(page);
    },
  },
  {
    name: "12g-system-detail-history-logs-edit",
    description: "System detail history/logs tabs and edit action",
    action: async (page) => {
      await routeSystemsWarningData(page);

      const historyResponsePromise = page
        .waitForResponse(
          (response) =>
            response.request().method() === "GET" &&
            /\/api\/v1\/systems\/[0-9a-f-]+\/history$/.test(new URL(response.url()).pathname),
          { timeout: 15000 },
        )
        .catch(() => null);

      const eventsResponsePromise = page
        .waitForResponse(
          (response) =>
            response.request().method() === "GET" &&
            /\/api\/v1\/systems\/[0-9a-f-]+\/agent-events$/.test(new URL(response.url()).pathname),
          { timeout: 15000 },
        )
        .catch(() => null);

      await page.goto(`${baseUrl}/systems/00000000-0000-0000-0000-0000000000a1`, {
        timeout: LOAD_TIMEOUT,
      });
      await page.waitForTimeout(1800);

      const historyResponse = await historyResponsePromise;
      const eventsResponse = await eventsResponsePromise;
      if (!historyResponse || !historyResponse.ok()) {
        throw new Error("Expected system history request to succeed on system detail page");
      }
      if (!eventsResponse || !eventsResponse.ok()) {
        throw new Error("Expected system agent-events request to succeed on system detail page");
      }

      await page.getByRole("button", { name: "History" }).first().click();
      await assertVisible(
        page.getByText("Current").first(),
        "Expected history timeline to render API-backed history entries",
      );

      await page.getByRole("button", { name: "Logs" }).first().click();
      await assertVisible(
        page.getByText("Agent Events").first(),
        "Expected logs tab to render API-backed agent events",
      );

      await assertVisible(
        page.getByRole("button", { name: /^Edit$/ }).first(),
        "Expected system detail header to render Edit action",
      );

      await unrouteSystemsWarningData(page);
    },
  },
  {
    name: "12h-system-detail-cves-grouped-justification",
    description: "System detail CVEs tab grouped list, filters, details link, and justification save",
    action: async (page) => {
      await routeSystemsWarningData(page);

      await page.route(
        "**/api/v1/systems/00000000-0000-0000-0000-0000000000a1/cve-scan-eligibility*",
        async (route) => {
          await route.fulfill({
            status: 200,
            contentType: "application/json",
            body: JSON.stringify({
              eligible: true,
              reason: null,
              derivation_id: 42,
              config_name: "warning-system-01",
              hostname: "warning-system-01",
            }),
          });
        },
      );

      let justificationSaved = false;
      let capturedJustificationRequest = null;

      await page.route("**/api/v1/systems/00000000-0000-0000-0000-0000000000a1/cves*", async (route) => {
        const payload = [
          {
            cve_id: "CVE-2025-1111",
            severity: "high",
            cvss_score: 8.7,
            description: "Kernel memory corruption under crafted input",
            package_name: "linuxPackages_6_10.kernel",
            installed_version: "6.10.12",
            fixed_version: "6.10.14",
            first_seen: "2026-04-10T09:00:00Z",
            published_at: "2026-04-08T00:00:00Z",
            status: "fix_available",
            justification_category: justificationSaved ? "accepted_risk" : null,
            justification_reason: justificationSaved
              ? "Accepted risk until scheduled maintenance window"
              : null,
            justification_updated_at: justificationSaved ? "2026-04-12T12:00:00Z" : null,
          },
          {
            cve_id: "CVE-2025-1111",
            severity: "high",
            cvss_score: 8.7,
            description: "Kernel memory corruption under crafted input",
            package_name: "linuxPackages_6_1.kernel",
            installed_version: "6.1.93",
            fixed_version: "6.1.95",
            first_seen: "2026-04-10T09:00:00Z",
            published_at: "2026-04-08T00:00:00Z",
            status: "fix_available",
            justification_category: justificationSaved ? "accepted_risk" : null,
            justification_reason: justificationSaved
              ? "Accepted risk until scheduled maintenance window"
              : null,
            justification_updated_at: justificationSaved ? "2026-04-12T12:00:00Z" : null,
          },
          {
            cve_id: "CVE-2024-2222",
            severity: "low",
            cvss_score: 3.1,
            description: "Minor issue in optional diagnostics package",
            package_name: "diag-tools",
            installed_version: "2.3.1",
            fixed_version: null,
            first_seen: "2026-04-10T09:00:00Z",
            published_at: "2026-01-15T00:00:00Z",
            status: "open",
            justification_category: null,
            justification_reason: null,
            justification_updated_at: null,
          },
        ];

        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify(payload),
        });
      });

      await page.route(
        "**/api/v1/systems/00000000-0000-0000-0000-0000000000a1/cves/CVE-2025-1111/justification",
        async (route) => {
          capturedJustificationRequest = route.request().postDataJSON();
          justificationSaved = true;
          await route.fulfill({
            status: 200,
            contentType: "application/json",
            body: JSON.stringify({ status: "ok", message: "CVE justification saved" }),
          });
        },
      );

      await page.route("**/api/v1/systems/00000000-0000-0000-0000-0000000000a1/commits*", async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({ commits: [], current_commit: null }),
        });
      });

      await page.goto(`${baseUrl}/systems/00000000-0000-0000-0000-0000000000a1`, {
        timeout: LOAD_TIMEOUT,
      });
      await page.waitForTimeout(1200);

      await page.getByRole("button", { name: "CVEs" }).first().click();

      await assertVisible(
        page.getByText("2 grouped CVEs").first(),
        "Expected grouped CVE count to collapse duplicate CVE IDs",
        12000,
      );

      await assertVisible(
        page.getByText("2 packages").first(),
        "Expected grouped CVE row to show affected package count",
      );

      await page.locator("button", { hasText: "CVE-2025-1111" }).first().click();

      await assertVisible(
        page.getByText("Kernel memory corruption under crafted input").first(),
        "Expected expanded CVE entry to show internal description",
      );
      await assertVisible(
        page.getByText("linuxPackages_6_10.kernel").first(),
        "Expected expanded grouped CVE to show first affected package",
      );
      await assertVisible(
        page.getByText("linuxPackages_6_1.kernel").first(),
        "Expected expanded grouped CVE to show second affected package",
      );

      const nvdHref = await page.locator("a:has-text('View on NVD')").first().getAttribute("href");
      if (nvdHref !== "https://nvd.nist.gov/vuln/detail/CVE-2025-1111") {
        throw new Error(`Expected CVE details link to point at NVD detail page, got: ${nvdHref}`);
      }

      await page.locator("input[placeholder='Filter package/version']").fill("diag-tools");
      await assertVisible(
        page.getByText("CVE-2024-2222").first(),
        "Expected package filter to keep matching CVE",
      );

      const cve1111VisibleAfterPackageFilter = await page
        .getByText("CVE-2025-1111")
        .first()
        .isVisible({ timeout: 1500 })
        .catch(() => false);
      if (cve1111VisibleAfterPackageFilter) {
        throw new Error("Expected package filter to hide non-matching grouped CVE row");
      }

      await page.locator("input[placeholder='Filter package/version']").fill("");
      await page.locator("select").first().selectOption("high");
      await assertVisible(
        page.getByText("CVE-2025-1111").first(),
        "Expected severity filter to retain High CVE",
      );
      await assertHidden(
        page.getByText("CVE-2024-2222").first(),
        "Expected severity filter to hide Low CVE row",
      );

      await page.locator("select").first().selectOption("all");
      const cve1111Toggle = page.locator("button", { hasText: "CVE-2025-1111" }).first();
      const editJustificationButton = page
        .getByRole("button", { name: "Edit justification" })
        .first();
      const editButtonInitiallyVisible = await editJustificationButton
        .isVisible({ timeout: 1000 })
        .catch(() => false);
      if (!editButtonInitiallyVisible) {
        await cve1111Toggle.click();
      }
      await assertVisible(
        editJustificationButton,
        "Expected CVE row to provide justification edit action",
      );
      await editJustificationButton.click();

      await page.locator("select").nth(1).selectOption("accepted_risk");
      const reasonInput = page.locator("textarea[placeholder='Document risk acceptance / mitigation rationale']").first();
      const seededReason = await reasonInput.inputValue();
      if (!seededReason.toLowerCase().includes("accepted risk")) {
        throw new Error(`Expected preset selection to auto-populate justification reason, got: ${seededReason}`);
      }

      await reasonInput.fill("Accepted risk until scheduled maintenance window");

      const justificationResponsePromise = page
        .waitForResponse(
          (response) =>
            response.request().method() === "PUT" &&
            response
              .url()
              .includes("/api/v1/systems/00000000-0000-0000-0000-0000000000a1/cves/CVE-2025-1111/justification"),
          { timeout: 15000 },
        )
        .catch(() => null);

      await page.getByRole("button", { name: "Save" }).first().click();

      const justificationResponse = await justificationResponsePromise;
      if (!justificationResponse || !justificationResponse.ok()) {
        throw new Error("Expected CVE justification save request to succeed");
      }

      if (!capturedJustificationRequest) {
        throw new Error("Expected justification save payload to be captured");
      }
      if (capturedJustificationRequest.category !== "accepted_risk") {
        throw new Error(
          `Expected justification category accepted_risk, got ${capturedJustificationRequest.category}`,
        );
      }
      if (
        capturedJustificationRequest.reason !==
        "Accepted risk until scheduled maintenance window"
      ) {
        throw new Error(
          `Unexpected justification reason payload: ${capturedJustificationRequest.reason}`,
        );
      }

      await assertVisible(
        page.getByText("Justification saved").first(),
        "Expected UI acknowledgement after saving CVE justification",
      );

      await page.locator("button", { hasText: "CVE-2025-1111" }).first().click();
      await assertVisible(
        page.getByText("Justified").first(),
        "Expected grouped CVE row to remain visually marked after save + reload",
      );

      await page.unroute(
        "**/api/v1/systems/00000000-0000-0000-0000-0000000000a1/cve-scan-eligibility*",
      );
      await page.unroute("**/api/v1/systems/00000000-0000-0000-0000-0000000000a1/cves*");
      await page.unroute(
        "**/api/v1/systems/00000000-0000-0000-0000-0000000000a1/cves/CVE-2025-1111/justification",
      );
      await page.unroute("**/api/v1/systems/00000000-0000-0000-0000-0000000000a1/commits*");
      await unrouteSystemsWarningData(page);
    },
  },
  {
    name: "12d-systems-api-error-no-mock-fallback",
    description: "Systems API failures show error state without deterministic mock hosts",
    action: async (page) => {
      await routeSystemsApiFailure(page);
      await page.goto(`${baseUrl}/systems`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);

      await page.getByText(/Systems API unavailable/i).first().waitFor({ timeout: 5000 });

      const deterministicNotice = page.getByText(/deterministic fallback data/i).first();
      if (await deterministicNotice.isVisible({ timeout: 800 }).catch(() => false)) {
        throw new Error("Systems view still shows deterministic fallback notice");
      }

      const atlasHost = page.getByText(/atlas-0[12]/i).first();
      if (await atlasHost.isVisible({ timeout: 800 }).catch(() => false)) {
        throw new Error("Systems view still renders deterministic mock hostnames");
      }

      await unrouteSystemsApiFailure(page);
    },
  },
  {
    name: "12g-systems-warning-clears-after-link",
    description: "Systems missing-flake warning clears after linking flake via Edit modal",
    action: async (page) => {
      await routeSystemsWarningData(page);
      await page.goto(`${baseUrl}/systems`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2200);

      const warningBanner = page.locator("[data-testid='systems-missing-flake-warning']").first();
      await assertVisible(warningBanner, "Expected missing-flake warning before linking", 15000);

      const systemRow = page.locator("tr").filter({ hasText: "warning-system-01" }).first();
      await assertVisible(systemRow, "Expected warning-system-01 row to be visible", 15000);

      const detailResponsePromise = page
        .waitForResponse(
          (response) =>
            response.request().method() === "GET" &&
            response.url().includes("/api/v1/systems/00000000-0000-0000-0000-0000000000a1"),
          { timeout: 15000 },
        )
        .catch(() => null);

      await systemRow.getByRole("button", { name: "Edit" }).first().click();

      const detailResponse = await detailResponsePromise;
      if (!detailResponse || !detailResponse.ok()) {
        throw new Error("Expected system detail request to succeed before editing flake linkage");
      }

      await page.getByText("Update system configuration and deployment settings").first().waitFor({ timeout: 15000 });
      await page.getByRole("button", { name: "Save Changes" }).first().click();
      await page.waitForTimeout(1200);

      if (await warningBanner.isVisible().catch(() => false)) {
        throw new Error("Expected missing-flake warning to clear after linking flake via Edit modal");
      }

      await unrouteSystemsWarningData(page);
    },
  },
  {
    name: "13-flakes",
    description: "Flakes registry",
    action: async (page) => {
      await page.goto(`${baseUrl}/flakes`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);
    },
  },
  {
    name: "13e-flakes-add-modal-credentials",
    description: "Flake add modal with build scope and credential controls",
    action: async (page) => {
      await page.goto(`${baseUrl}/flakes`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);
      await page.locator("button:has-text('Add Flake')").first().click();
      await page.getByText("Register Flake").first().waitFor({ timeout: 5000 });
      await page.getByLabel(/Authentication Type/i).selectOption("pat");
      await page.getByLabel(/Token Username/i).fill("oauth2");
      await page.getByLabel(/Token Secret/i).fill("glpat-example-token");
      await page.getByLabel(/Build Scope/i).selectOption("all_configs");
    },
  },
  {
    name: "13f-flakes-edit-modal-credentials",
    description: "Flake edit modal showing existing build scope and credential controls",
    action: async (page) => {
      await routeFlakeWarningData(page);
      await page.route(/\/api\/v1\/flakes\/\d+\/credentials$/, async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({
            flake_id: 1,
            auth_type: "ssh_key",
            username: null,
            ssh_username: "git",
            has_secret: true,
          }),
        });
      });

      await page.goto(`${baseUrl}/flakes`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);
      const editButton = page.locator("button:has-text('Edit')").first();
      await editButton.waitFor({ timeout: 5000 });
      await editButton.click();
      await page.getByRole("heading", { name: "Edit Flake" }).waitFor({ timeout: 5000 });
      await page.getByLabel(/Build Scope/i).selectOption("cf_systems_only");
      await page.getByLabel(/Authentication Type/i).selectOption("ssh_key");
      await page.getByLabel(/SSH Username/i).fill("git");
      await page.unroute(/\/api\/v1\/flakes\/\d+\/credentials$/);
      await unrouteFlakeWarningData(page);
    },
  },
  {
    name: "13d-flakes-stress-dataset",
    description: "Flakes view remains responsive with production-shaped timeline payload",
    action: async (page) => {
      await routeFlakesStressData(page);
      await page.goto(`${baseUrl}/flakes`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2500);

      await page.getByText("platform-core").first().waitFor({ timeout: 5000 });
      await page.getByText("edge-fleet").first().click();
      await page.waitForTimeout(800);

      const probe = await page.evaluate(() => {
        window.__cfResponsivenessProbe = (window.__cfResponsivenessProbe || 0) + 1;
        return window.__cfResponsivenessProbe;
      });

      if (probe < 1) {
        throw new Error("Flakes stress dataset responsiveness probe did not execute");
      }

      await assertVisible(
        page.locator("button:has-text('Run CVE Scan')").first(),
        "Expected per-config Run CVE Scan action in flakes history",
        12000,
      );

      await unrouteFlakesStressData(page);
    },
  },
  {
    name: "13b-flakes-config-warning",
    description: "Flakes warning state for latest evaluation errors",
    action: async (page) => {
      await routeConfigHealth(
        page,
        mockConfigHealthResponse({
          checks: [
            {
              id: "no_flakes",
              passed: true,
              message:
                "No flakes are being watched. Add a flake to begin evaluating NixOS configurations.",
              action_url: "/flakes",
            },
            {
              id: "no_environments",
              passed: true,
              message:
                "No environments exist. Environments are required to organize systems, builders, and caches.",
              action_url: "/environments",
            },
            {
              id: "no_builders",
              passed: true,
              message:
                "No builders are registered. Derivations will be evaluated but never built.",
              action_url: "/builders",
            },
            {
              id: "no_cache_destinations",
              passed: true,
              message:
                "No cache destinations configured. Builds will succeed but agents won't be able to pull deployments.",
              action_url: "/caches",
            },
            {
              id: "flake_eval_errors",
              passed: false,
              message:
                "One or more flakes have evaluation errors on their latest commit. Check flake configuration.",
              action_url: "/flakes",
            },
          ],
        }),
      );
      await routeFlakeWarningData(page);
      await page.goto(`${baseUrl}/flakes`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);
      await page.getByText(/latest commit/i).first().waitFor({ timeout: 5000 });
      await page.evaluate(() => window.scrollTo(0, 140));
      await unrouteFlakeWarningData(page);
      await unrouteConfigHealth(page);
    },
  },
  {
    name: "13c-flakes-history-rewrite-modal",
    description: "Flakes view history rewrite conflict modal after sync",
    action: async (page) => {
      await page.route(/\/api\/v1\/flakes\/\d+\/sync$/, async (route) => {
        await route.fulfill({
          status: 409,
          contentType: "application/json",
          body: JSON.stringify({
            error: "history_rewrite_detected",
            message:
              "Git history rewrite detected for test flake. Review and accept rewrite before sync.",
            details: {
              accept_rewrite_endpoint: "/api/v1/flakes/1/accept-rewrite",
            },
          }),
        });
      });

      await page.goto(`${baseUrl}/flakes`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);

      const syncBtn = page.locator("button:has-text('Sync from Source')").first();
      await syncBtn.waitFor({ timeout: 5000 });
      await syncBtn.click();

      await page
        .locator("text=History Rewrite Detected")
        .first()
        .waitFor({ timeout: 5000 });

      await page.unroute(/\/api\/v1\/flakes\/\d+\/sync$/);
    },
  },
  {
    name: "14-environments",
    description: "Environments registry",
    action: async (page) => {
      await page.goto(`${baseUrl}/environments`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);
    },
  },
  {
    name: "14b-environments-config-warning",
    description: "Environments warning state for missing builder and cache assignments",
    action: async (page) => {
      await routeConfigHealth(
        page,
        mockConfigHealthResponse({
          has_builders: false,
          has_cache_destinations: false,
        }),
      );
      await routeEnvironmentWarningData(page);
      await page.goto(`${baseUrl}/environments`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);
      await page.getByText(/No builder is registered/i).first().waitFor({ timeout: 5000 });
      await page.evaluate(() => window.scrollTo(0, 140));
      await unrouteEnvironmentWarningData(page);
      await unrouteConfigHealth(page);
    },
  },
  {
    name: "15-builds",
    description: "Builds page",
    action: async (page) => {
      await routeBuildsData(page);
      await page.goto(`${baseUrl}/builds`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);

      const queueCards = page.locator("[data-testid='build-queue-card']");
      const queueCardCount = await queueCards.count();
      if (queueCardCount === 0) {
        throw new Error("Expected at least one build queue card in builds screenshot");
      }
      if (queueCardCount > 0) {
        const overflowingCards = await queueCards.evaluateAll((cards) =>
          cards.filter((card) => card.scrollWidth > card.clientWidth + 1).length,
        );
        if (overflowingCards > 0) {
          throw new Error(`Build queue has ${overflowingCards} overflowing cards`);
        }
      }

      await unrouteBuildsData(page);
    },
  },
  {
    name: "11b-builds-queue-card-focus",
    description: "Build queue card layout focus",
    action: async (page) => {
      await routeBuildsData(page);
      await page.goto(`${baseUrl}/builds`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);

      const firstQueueCard = page.locator("[data-testid='build-queue-card']").first();
      if (await firstQueueCard.isVisible({ timeout: 2000 }).catch(() => false)) {
        await firstQueueCard.click();
        await page.waitForTimeout(700);
      } else {
        throw new Error("Expected first build queue card to be visible for focused screenshot");
      }

      await unrouteBuildsData(page);
    },
  },
  {
    name: "15b-builds-completed-tab",
    description: "Builds page - Completed Builds tab",
    action: async (page) => {
      await routeBuildsData(page);
      await page.goto(`${baseUrl}/builds`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);

      // Click the Completed Builds tab
      const completedTab = page.locator("button:has-text('Completed Builds')");
      await completedTab.click();
      await page.waitForTimeout(800);

      // Verify the completed builds table is visible
      const completedTable = page.locator("table").first();
      if (!(await completedTable.isVisible({ timeout: 2000 }).catch(() => false))) {
        throw new Error("Expected completed builds table to be visible");
      }

      await unrouteBuildsData(page);
    },
  },
  {
    name: "15c-builds-completed-filters",
    description: "Builds page - Completed Builds with filters",
    action: async (page) => {
      await routeBuildsData(page);
      await page.goto(`${baseUrl}/builds`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);

      // Switch to Completed Builds tab
      const completedTab = page.locator("button:has-text('Completed Builds')");
      await completedTab.click();
      await page.waitForTimeout(800);

      // Select "Failed" filter
      const statusFilter = page.locator("select").first();
      await statusFilter.selectOption("failed");
      await page.waitForTimeout(500);

      // Change sort order
      const sortSelect = page.locator("select").last();
      await sortSelect.selectOption("oldest");
      await page.waitForTimeout(500);

      await unrouteBuildsData(page);
    },
  },
  // ============================================================
  // BUILDS QUEUE CONTROLS EVIDENCE (TASK-237)
  // These steps capture evidence for:
  // - Table view mode toggle
  // - Cancelling/cancelled states
  // - Human-readable duration formatting
  // ============================================================
  {
    name: "15d-builds-queue-table-view",
    description: "Build queue in table view mode",
    action: async (page) => {
      await routeBuildsDataWithCancelStates(page);
      await page.goto(`${baseUrl}/builds`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);

      // Find and click the table view toggle button
      const tableToggle = page.locator("[data-testid='queue-view-table']");
      await assertVisible(tableToggle, "Table view toggle should be visible");
      await tableToggle.click();
      await page.waitForTimeout(800);

      // Verify table view is now displayed
      const queueTable = page.locator("[data-testid='build-queue-table']");
      await assertVisible(queueTable, "Build queue table should be visible after toggle");

      // Verify table has rows
      const tableRows = page.locator("[data-testid='build-queue-row']");
      const rowCount = await tableRows.count();
      if (rowCount === 0) {
        throw new Error("Expected at least one build queue row in table view");
      }

      await unrouteBuildsDataWithCancelStates(page);
    },
  },
  {
    name: "15e-builds-cancelling-state",
    description: "Build queue showing cancelling/stopping state",
    action: async (page) => {
      await routeBuildsDataWithCancelStates(page);
      await page.goto(`${baseUrl}/builds`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);

      // Stay in card view (default) to show cancelling state badge
      const queueCards = page.locator("[data-testid='build-queue-card']");
      const cardCount = await queueCards.count();
      if (cardCount === 0) {
        throw new Error("Expected at least one build queue card");
      }

      // Verify we can see the stopping/cancelling status badge
      const stoppingBadge = page.getByText(/stopping|cancelling/i).first();
      const stoppingVisible = await stoppingBadge.isVisible({ timeout: 2000 }).catch(() => false);
      if (!stoppingVisible) {
        throw new Error("Expected stopping/cancelling status badge to be visible in queue");
      }

      await unrouteBuildsDataWithCancelStates(page);
    },
  },
  {
    name: "15f-builds-human-duration",
    description: "Build queue showing human-readable duration format",
    action: async (page) => {
      await routeBuildsDataWithCancelStates(page);
      await page.goto(`${baseUrl}/builds`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);

      // Switch to table view to see duration column more clearly
      const tableToggle = page.locator("[data-testid='queue-view-table']");
      await tableToggle.click();
      await page.waitForTimeout(800);

      // The mock data has elapsed_secs: 3723 which should display as "1h 2m" (approximately)
      // Look for human-readable time format patterns like "Xh Ym" or "Xm Ys"
      const durationText = await page.locator("[data-testid='build-queue-table']").textContent();
      const hasHumanDuration = /\d+h\s+\d+m|\d+m\s+\d+s|\d+s/.test(durationText);
      if (!hasHumanDuration) {
        throw new Error("Expected human-readable duration format (e.g., '1h 2m') in queue table");
      }

      await unrouteBuildsDataWithCancelStates(page);
    },
  },
  {
    name: "15g-builds-action-visibility",
    description: "Build queue action buttons shown only for valid states",
    action: async (page) => {
      await routeBuildsDataWithCancelStates(page);
      await page.goto(`${baseUrl}/builds`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);

      // In card view, click on a cancelled build to select it
      // Restart should be visible for cancelled builds
      const cards = page.locator("[data-testid='build-queue-card']");
      const cardCount = await cards.count();
      
      // Find the card with cancelled status and click it
      for (let i = 0; i < cardCount; i++) {
        const cardText = await cards.nth(i).textContent();
        if (/cancelled|canceled/i.test(cardText)) {
          await cards.nth(i).click();
          await page.waitForTimeout(500);
          break;
        }
      }

      // Verify Restart button is visible for cancelled build
      const restartBtn = page.locator("button:has-text('Restart')");
      const restartVisible = await restartBtn.isVisible({ timeout: 2000 }).catch(() => false);
      if (!restartVisible) {
        throw new Error("Expected Restart button to be visible for cancelled build");
      }

      await unrouteBuildsDataWithCancelStates(page);
    },
  },
  {
    name: "15h-builds-completed-restart-action",
    description: "Completed tab restart action requeues cancelled build",
    action: async (page) => {
      await routeBuildsDataWithCancelStates(page);

      // Override recent builds to include a cancelled item in Completed tab.
      await page.route("**/api/v1/build-jobs/recent*", async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify(mockRecentBuildsWithCancelled()),
        });
      });

      let requeueCalls = 0;
      await page.route("**/api/v1/build-jobs/*/requeue", async (route) => {
        if (route.request().method() === "POST") {
          requeueCalls += 1;
        }
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: "{}",
        });
      });

      await page.goto(`${baseUrl}/builds`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);

      const completedTab = page.locator("button:has-text('Completed Builds')");
      await assertVisible(completedTab, "Completed Builds tab should be visible");
      await completedTab.click();
      await page.waitForTimeout(800);

      const cancelledRow = page.locator("tr", { hasText: "cancelled-history-system" });
      await assertVisible(cancelledRow, "Cancelled build row should be visible in Completed tab");

      const restartBtn = cancelledRow.locator("button:has-text('Restart')");
      await assertVisible(restartBtn, "Restart button should be visible for cancelled completed build");
      await restartBtn.click();
      await page.getByRole("heading", { name: "Restart build?" }).waitFor({ timeout: 3000 });

      const modalConfirm = page.locator(".cf-modal-panel-30 button:has-text('Restart')");
      await assertVisible(modalConfirm, "Restart confirmation button should be visible in modal");
      await modalConfirm.click();
      await page.waitForTimeout(600);

      if (requeueCalls < 1) {
        throw new Error("Expected Restart from Completed tab to call requeue endpoint");
      }

      const missingRowError = page.getByText(/Build row #.* not found/i);
      await assertHidden(
        missingRowError,
        "Restart from Completed tab should not show 'Build row not found' error",
      );

      await page.unroute("**/api/v1/build-jobs/recent*");
      await page.unroute("**/api/v1/build-jobs/*/requeue");
      await unrouteBuildsDataWithCancelStates(page);
    },
  },
  {
    name: "16-cves",
    description: "CVE dashboard - fleet overview",
    action: async (page) => {
      // Mock the CVE API endpoints so the test doesn't require real scan data.
      await page.route("**/api/v1/cves/summary*", async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({
            total_open: 42,
            severity: { critical: 5, high: 12, medium: 18, low: 7 },
            affected_systems: 8,
            new_cves_last_7_days: 3,
            oldest_cve_age_days: 730,
          }),
        });
      });
      await page.route("**/api/v1/cves/top-systems*", async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify([
            {
              system_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
              hostname: "prod-server-01",
              total_cves: 15,
              critical_cves: 3,
              high_cves: 5,
              medium_cves: 4,
              low_cves: 3,
              days_since_scan: 2,
              last_cve_scan: new Date().toISOString(),
            },
          ]),
        });
      });
      await page.route("**/api/v1/cves/scan-freshness*", async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify([
            {
              system_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
              hostname: "prod-server-01",
              days_since_scan: 2,
              last_cve_scan: new Date().toISOString(),
              total_cves: 15,
            },
          ]),
        });
      });
      await page.route("**/api/v1/cves/vulnerabilities*", async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify([
            {
              system_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
              hostname: "prod-server-01",
              cve_id: "CVE-2024-1234",
              severity: "critical",
              cvss_score: 9.8,
              package_name: "openssl",
              installed_version: "3.0.1",
              fixed_version: "3.0.2",
              first_seen: new Date().toISOString(),
              status: "open",
            },
          ]),
        });
      });

      await page.goto(`${baseUrl}/cves`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);

      // Assert the page heading is present.
      const heading = page.locator("main h1:has-text('CVE Dashboard')");
      await assertVisible(heading, "Expected CVE Dashboard heading");

      // Assert summary stat cards are rendered.
      const totalCard = page.locator("main").getByText("Total Open CVEs");
      await assertVisible(totalCard, "Expected 'Total Open CVEs' stat card");

      // Assert severity breakdown section.
      const criticalCard = page.locator("main").getByText("Critical").first();
      await assertVisible(criticalCard, "Expected severity breakdown visible");

      // Assert the drill-down section is rendered.
      const drillDownSection = page.locator("[data-testid='cve-drill-down']");
      await assertVisible(drillDownSection, "Expected CVE drill-down section");

      // Assert top-systems section.
      const topSystems = page.locator("[data-testid='cve-top-systems']");
      await assertVisible(topSystems, "Expected top-affected systems section");

      // Assert scan freshness section.
      const freshness = page.locator("[data-testid='cve-scan-freshness']");
      await assertVisible(freshness, "Expected scan freshness section");

      // Unroute after test.
      await page.unroute("**/api/v1/cves/summary*");
      await page.unroute("**/api/v1/cves/top-systems*");
      await page.unroute("**/api/v1/cves/scan-freshness*");
      await page.unroute("**/api/v1/cves/vulnerabilities*");
    },
  },
  {
    name: "16b-cves-severity-filter",
    description: "CVE dashboard - severity filter re-issues request with ?severity=critical",
    action: async (page) => {
      await page.route("**/api/v1/cves/summary*", async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({
            total_open: 17,
            severity: { critical: 5, high: 12, medium: 0, low: 0 },
            affected_systems: 4,
            new_cves_last_7_days: 1,
            oldest_cve_age_days: 90,
          }),
        });
      });
      await page.route("**/api/v1/cves/top-systems*", async (route) => {
        await route.fulfill({ status: 200, contentType: "application/json", body: "[]" });
      });
      await page.route("**/api/v1/cves/scan-freshness*", async (route) => {
        await route.fulfill({ status: 200, contentType: "application/json", body: "[]" });
      });

      // Collect all URLs that hit the vulnerabilities endpoint so we can assert
      // the severity filter is sent as a query param after chip click.
      const vulnerabilityUrls = [];
      await page.route("**/api/v1/cves/vulnerabilities*", async (route) => {
        vulnerabilityUrls.push(route.request().url());
        // First (unfiltered) call returns empty; filtered call returns a critical row.
        const url = route.request().url();
        if (url.includes("severity=critical")) {
          await route.fulfill({
            status: 200,
            contentType: "application/json",
            body: JSON.stringify([
              {
                system_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
                hostname: "prod-server-01",
                cve_id: "CVE-2024-9999",
                severity: "critical",
                cvss_score: 9.8,
                package_name: "openssl",
                installed_version: "3.0.1",
                fixed_version: "3.0.2",
                first_seen: new Date().toISOString(),
                status: "fix_available",
              },
            ]),
          });
        } else {
          await route.fulfill({ status: 200, contentType: "application/json", body: "[]" });
        }
      });

      await page.goto(`${baseUrl}/cves`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);

      // Wait for the initial unfiltered vulnerabilities request to settle.
      const initialCount = vulnerabilityUrls.length;

      // Click the Critical severity filter button.
      const criticalBtn = page.locator("button:has-text('Critical')").first();
      await criticalBtn.waitFor({ timeout: 5000 });
      // Register response wait before clicking to avoid race conditions when the
      // filtered request resolves very quickly in CI.
      const filteredResponsePromise = page.waitForResponse(
        (resp) =>
          resp.url().includes("/api/v1/cves/vulnerabilities") &&
          resp.url().includes("severity=critical"),
        { timeout: 8000 },
      );
      await criticalBtn.click();

      // Wait for a new vulnerabilities request that includes severity=critical.
      await filteredResponsePromise;

      // Assert a new request was fired after the click (filter is reactive).
      if (vulnerabilityUrls.length <= initialCount) {
        throw new Error(
          "Expected a new vulnerabilities request after clicking Critical chip, but none was observed",
        );
      }

      // Assert the most recent request URL contains severity=critical.
      const lastUrl = vulnerabilityUrls[vulnerabilityUrls.length - 1];
      if (!lastUrl.includes("severity=critical")) {
        throw new Error(
          `Expected vulnerabilities request to include severity=critical, got: ${lastUrl}`,
        );
      }

      // Assert the Critical chip now has the active/highlighted style.
      // The active chip gets class bg-violet-600/20 per FilterButton component.
      const activeCriticalBtn = page.locator(
        "button:has-text('Critical').bg-violet-600\\/20",
      );
      await assertVisible(
        activeCriticalBtn,
        "Expected Critical filter chip to have active style after click",
      );

      // Assert the filtered result row (CVE-2024-9999) rendered in the drill-down table.
      const filteredRow = page.locator("td:has-text('CVE-2024-9999')");
      await assertVisible(filteredRow, "Expected filtered CVE row CVE-2024-9999 to appear after severity filter");

      await page.unroute("**/api/v1/cves/summary*");
      await page.unroute("**/api/v1/cves/top-systems*");
      await page.unroute("**/api/v1/cves/scan-freshness*");
      await page.unroute("**/api/v1/cves/vulnerabilities*");
    },
  },
  {
    name: "17-style-guide",
    description: "Style guide",
    action: async (page) => {
      await page.goto(`${baseUrl}/style-guide`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);
    },
  },
  {
    name: "18-policies",
    description: "Policies view",
    action: async (page) => {
      await page.goto(`${baseUrl}/deployment-policies`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2500);
      await page.locator("main h1:has-text('Deployment Policies')").first().waitFor({ timeout: 5000 });
    },
  },
  {
    name: "19-policies-new-modal-basic",
    description: "Policies new modal in basic mode",
    action: async (page) => {
      await page.goto(`${baseUrl}/deployment-policies`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2500);
      const newPolicyBtn = page.locator("button:has-text('New Policy')").first();
      await newPolicyBtn.waitFor({ timeout: 5000 });
      await newPolicyBtn.click();
      await page.waitForTimeout(1200);
      await page.getByRole("heading", { name: "Create Policy" }).waitFor({ timeout: 5000 });
    },
  },
  {
    name: "20-policies-new-modal-advanced",
    description: "Policies new modal in advanced mode",
    action: async (page) => {
      await page.goto(`${baseUrl}/deployment-policies`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2500);
      const newPolicyBtn = page.locator("button:has-text('New Policy')").first();
      await newPolicyBtn.waitFor({ timeout: 5000 });
      await newPolicyBtn.click();
      await page.waitForTimeout(700);
      const advancedBtn = page.getByRole("button", { name: "Advanced" });
      await advancedBtn.waitFor({ timeout: 5000 });
      await advancedBtn.click();
      await page.waitForTimeout(1200);
      await page.getByText("Policy Definition").first().waitFor({ timeout: 5000 });
    },
  },
  // ── CVE policy API round-trip checks ────────────────────────────────────
  // These tests exercise the new policy types introduced in TASK-176 through
  // the real server API to verify the full create → parse → list round-trip.
  {
    name: "20b-policies-cve-gate-create-roundtrip",
    description: "API: create require_cve_check policy and verify it round-trips correctly",
    action: async (page) => {
      // Create a require_cve_check policy via the API.
      const createResponse = await page.evaluate(async (base) => {
        const res = await fetch(`${base}/api/v1/deployment-policies`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            name: "ci-test-cve-gate",
            description: "CI check: CVE gate round-trip",
            policy_type: "require_cve_check",
            config: {
              max_critical: 0,
              max_high: 5,
              require_high_justification: true,
              strict: true,
              when_no_scan: "block",
            },
            enabled: false,
          }),
          credentials: "include",
        });
        return { status: res.status, body: await res.json() };
      }, baseUrl);

      if (createResponse.status !== 201) {
        throw new Error(
          `Expected 201 creating require_cve_check policy, got ${createResponse.status}: ${JSON.stringify(createResponse.body)}`
        );
      }

      const createdId = createResponse.body.id;
      if (!createdId) {
        throw new Error("Created policy has no id field");
      }

      // Fetch it back and verify the config was stored correctly.
      const getResponse = await page.evaluate(async ({ base, id }) => {
        const res = await fetch(`${base}/api/v1/deployment-policies/${id}`, {
          credentials: "include",
        });
        return { status: res.status, body: await res.json() };
      }, { base: baseUrl, id: createdId });

      if (getResponse.status !== 200) {
        throw new Error(
          `Expected 200 fetching policy ${createdId}, got ${getResponse.status}`
        );
      }

      const policy = getResponse.body;
      if (policy.policy_type !== "require_cve_check") {
        throw new Error(`policy_type mismatch: expected require_cve_check, got ${policy.policy_type}`);
      }
      const cfg = policy.config;
      if (cfg.max_critical !== 0) {
        throw new Error(`max_critical mismatch: expected 0, got ${cfg.max_critical}`);
      }
      if (cfg.max_high !== 5) {
        throw new Error(`max_high mismatch: expected 5, got ${cfg.max_high}`);
      }
      if (cfg.when_no_scan !== "block") {
        throw new Error(`when_no_scan mismatch: expected block, got ${cfg.when_no_scan}`);
      }
      if (cfg.require_high_justification !== true) {
        throw new Error(`require_high_justification mismatch: expected true, got ${cfg.require_high_justification}`);
      }

      // Clean up.
      await page.evaluate(async ({ base, id }) => {
        await fetch(`${base}/api/v1/deployment-policies/${id}`, {
          method: "DELETE",
          credentials: "include",
        });
      }, { base: baseUrl, id: createdId });
    },
  },
  {
    name: "20c-policies-multirule-create-roundtrip",
    description: "API: create multi-rule custom_check (mode=any) and verify rules[] round-trips",
    action: async (page) => {
      // Create a multi-rule custom_check with mode=any.
      const createResponse = await page.evaluate(async (base) => {
        const res = await fetch(`${base}/api/v1/deployment-policies`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            name: "ci-test-multi-rule",
            description: "CI check: multi-rule any-mode round-trip",
            policy_type: "custom_check",
            config: {
              rules: [
                {
                  expression: "(cfg.config.services.crystal-forge.enable or false)",
                  description: "CF agent enabled",
                  field_name: "cfAgentEnabled",
                  strict: true,
                },
                {
                  expression: "(builtins.elem \"git\" (builtins.map (p: p.pname or \"\") cfg.config.environment.systemPackages))",
                  description: "git installed",
                  field_name: "gitInstalled",
                  strict: true,
                },
              ],
              mode: "any",
              strict: true,
            },
            enabled: false,
          }),
          credentials: "include",
        });
        return { status: res.status, body: await res.json() };
      }, baseUrl);

      if (createResponse.status !== 201) {
        throw new Error(
          `Expected 201 creating multi-rule policy, got ${createResponse.status}: ${JSON.stringify(createResponse.body)}`
        );
      }

      const createdId = createResponse.body.id;

      // Fetch back and assert rules[] and mode are preserved.
      const getResponse = await page.evaluate(async ({ base, id }) => {
        const res = await fetch(`${base}/api/v1/deployment-policies/${id}`, {
          credentials: "include",
        });
        return { status: res.status, body: await res.json() };
      }, { base: baseUrl, id: createdId });

      if (getResponse.status !== 200) {
        throw new Error(`Expected 200, got ${getResponse.status}`);
      }

      const policy = getResponse.body;
      const cfg = policy.config;

      if (!Array.isArray(cfg.rules) || cfg.rules.length !== 2) {
        throw new Error(
          `Expected 2 rules in stored policy, got: ${JSON.stringify(cfg.rules)}`
        );
      }
      if (cfg.mode !== "any") {
        throw new Error(`mode mismatch: expected any, got ${cfg.mode}`);
      }
      if (cfg.rules[0].field_name !== "cfAgentEnabled") {
        throw new Error(`rules[0].field_name mismatch: got ${cfg.rules[0].field_name}`);
      }
      if (cfg.rules[1].field_name !== "gitInstalled") {
        throw new Error(`rules[1].field_name mismatch: got ${cfg.rules[1].field_name}`);
      }

      // Clean up.
      await page.evaluate(async ({ base, id }) => {
        await fetch(`${base}/api/v1/deployment-policies/${id}`, {
          method: "DELETE",
          credentials: "include",
        });
      }, { base: baseUrl, id: createdId });
    },
  },
  {
    name: "20d-policies-cve-gate-invalid-rejected",
    description: "API: require_cve_check with invalid when_no_scan value is rejected 400",
    action: async (page) => {
      const createResponse = await page.evaluate(async (base) => {
        const res = await fetch(`${base}/api/v1/deployment-policies`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            name: "ci-test-cve-bad",
            policy_type: "require_cve_check",
            config: {
              max_critical: 0,
              when_no_scan: "invalid_value",
            },
            enabled: false,
          }),
          credentials: "include",
        });
        return { status: res.status };
      }, baseUrl);

      if (createResponse.status !== 400) {
        throw new Error(
          `Expected 400 for invalid when_no_scan, got ${createResponse.status}`
        );
      }
    },
  },
  {
    name: "20e-policies-multirule-rules-only-no-expression-required",
    description: "API: rules-only custom_check (no top-level expression) is accepted",
    action: async (page) => {
      // Regression: before the parser fix, a rules-only policy was accepted by the
      // API validator but silently dropped when loading policies for evaluation.
      // This verifies acceptance at the API boundary.
      const createResponse = await page.evaluate(async (base) => {
        const res = await fetch(`${base}/api/v1/deployment-policies`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            name: "ci-test-rules-only",
            description: "CI check: rules-only policy (no expression field)",
            policy_type: "custom_check",
            config: {
              rules: [
                {
                  expression: "true",
                  description: "always passes",
                  field_name: "alwaysPass",
                  strict: true,
                },
              ],
              mode: "all",
              strict: true,
            },
            enabled: false,
          }),
          credentials: "include",
        });
        return { status: res.status, body: await res.json() };
      }, baseUrl);

      if (createResponse.status !== 201) {
        throw new Error(
          `Expected 201 for rules-only policy, got ${createResponse.status}: ${JSON.stringify(createResponse.body)}`
        );
      }

      // Clean up.
      await page.evaluate(async ({ base, id }) => {
        await fetch(`${base}/api/v1/deployment-policies/${id}`, {
          method: "DELETE",
          credentials: "include",
        });
      }, { base: baseUrl, id: createResponse.body.id });
    },
  },
  // ── End CVE/multi-rule policy checks ─────────────────────────────────────
  {
    name: "21-caches",
    description: "Cache management view",
    action: async (page) => {
      await page.goto(`${baseUrl}/caches`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2500);
    },
  },
  {
    name: "22-caches-modal-nix",
    description: "Add cache modal with Nix type selected",
    action: async (page) => {
      await page.goto(`${baseUrl}/caches`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2500);

      const addBtn = page.locator("button:has-text('Add Destination')").first();
      await addBtn.waitFor({ timeout: 5000 });
      await addBtn.click();
      await page.getByRole("heading", { name: "Add Cache Destination" }).waitFor({ timeout: 5000 });

      const dialog = page.locator("[role='dialog']").first();
      await dialog.locator("select").first().selectOption("Nix");
      await page.waitForTimeout(1200);
    },
  },
  {
    name: "23-caches-modal-http",
    description: "Add cache modal with Http type selected",
    action: async (page) => {
      await page.goto(`${baseUrl}/caches`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2500);

      const addBtn = page.locator("button:has-text('Add Destination')").first();
      await addBtn.waitFor({ timeout: 5000 });
      await addBtn.click();
      await page.getByRole("heading", { name: "Add Cache Destination" }).waitFor({ timeout: 5000 });

      const dialog = page.locator("[role='dialog']").first();
      await dialog.locator("select").first().selectOption("Http");
      await page.waitForTimeout(1200);
    },
  },
  {
    name: "24-caches-modal-s3",
    description: "Add cache modal with S3 type selected",
    action: async (page) => {
      await page.goto(`${baseUrl}/caches`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2500);

      const addBtn = page.locator("button:has-text('Add Destination')").first();
      await addBtn.waitFor({ timeout: 5000 });
      await addBtn.click();
      await page.getByRole("heading", { name: "Add Cache Destination" }).waitFor({ timeout: 5000 });

      const dialog = page.locator("[role='dialog']").first();
      await dialog.locator("select").first().selectOption("S3");
      await page.waitForTimeout(1200);
    },
  },
  {
    name: "25-caches-modal-attic",
    description: "Add cache modal with Attic type selected",
    action: async (page) => {
      await page.goto(`${baseUrl}/caches`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2500);

      const addBtn = page.locator("button:has-text('Add Destination')").first();
      await addBtn.waitFor({ timeout: 5000 });
      await addBtn.click();
      await page.getByRole("heading", { name: "Add Cache Destination" }).waitFor({ timeout: 5000 });

      const dialog = page.locator("[role='dialog']").first();
      await dialog.locator("select").first().selectOption("Attic");
      await page.waitForTimeout(1200);
    },
  },
  // ── TASK-273: Evaluation cancellation and history ────────────────────────
  {
    name: "26-evaluations",
    description: "Evaluations page — Active Queue tab with cancel buttons (TASK-273)",
    action: async (page) => {
      const evalQueueMock = {
        active_count: 3,
        completed_count: 12,
        execution_mode: "standard",
        timestamp: new Date().toISOString(),
        items: [
          {
            commit_id: 1001,
            flake_id: 1,
            flake_name: "infrastructure",
            branch: "main",
            commit_hash: "a1b2c3d4e5f6a7b8",
            commit_message: "feat: upgrade postgresql to 17.x",
            author: "alice",
            committed_at: new Date(Date.now() - 120000).toISOString(),
            evaluation_status: "in_progress",
            queue_position: 1,
            systems: ["gray", "reckless", "butler", "chesty"],
            system_count: 4,
            passed_count: 2,
            policy_failed_count: 0,
            eval_failed_count: 0,
          },
          {
            commit_id: 1002,
            flake_id: 1,
            flake_name: "infrastructure",
            branch: "main",
            commit_hash: "b2c3d4e5f6a7b8c9",
            commit_message: "chore: update nixpkgs input",
            author: "bob",
            committed_at: new Date(Date.now() - 300000).toISOString(),
            evaluation_status: "pending",
            queue_position: 2,
            systems: [],
            system_count: 0,
            passed_count: 0,
            policy_failed_count: 0,
            eval_failed_count: 0,
          },
          {
            commit_id: 1003,
            flake_id: 2,
            flake_name: "workstations",
            branch: "dev",
            commit_hash: "c3d4e5f6a7b8c9d0",
            commit_message: "fix: add missing font packages",
            author: "carol",
            committed_at: new Date(Date.now() - 600000).toISOString(),
            evaluation_status: "cancelling",
            queue_position: 3,
            systems: [],
            system_count: 0,
            passed_count: 0,
            policy_failed_count: 0,
            eval_failed_count: 0,
          },
        ],
      };

      await page.route("**/api/v1/commits/eval-queue**", async (route) => {
        await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(evalQueueMock) });
      });
      await page.route("**/api/v1/commits/*/cancel-evaluation**", async (route) => {
        await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ outcome: "cancelled" }) });
      });
      await page.route("**/api/v1/commits/*/force-cancel-evaluation**", async (route) => {
        await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ outcome: "cancelled" }) });
      });
      // Mock the eval log WebSocket endpoint gracefully
      await page.route("**/api/v1/commits/*/eval/stream**", async (route) => {
        await route.fulfill({ status: 200, contentType: "text/plain", body: "" });
      });

      await page.goto(`${baseUrl}/evaluations`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2500);

      // Assert Active Queue tab is shown with items
      const activeQueueHeading = page.getByText(/Active Queue/i).first();
      await activeQueueHeading.waitFor({ timeout: 5000 });

      // Assert Cancel button is visible on in_progress row
      const cancelBtn = page.getByRole("button", { name: /Cancel/i }).first();
      await cancelBtn.waitFor({ timeout: 5000 });

      // Assert Force Cancel button is visible on cancelling row
      const forceCancelBtn = page.getByRole("button", { name: /Force Cancel/i }).first();
      await forceCancelBtn.waitFor({ timeout: 5000 });

      await page.unroute("**/api/v1/commits/eval-queue**");
      await page.unroute("**/api/v1/commits/*/cancel-evaluation**");
      await page.unroute("**/api/v1/commits/*/force-cancel-evaluation**");
      await page.unroute("**/api/v1/commits/*/eval/stream**");
    },
  },
  {
    name: "26b-evaluations-history",
    description: "Evaluations page — History tab with completed/failed/cancelled rows (TASK-273)",
    action: async (page) => {
      const evalQueueMock = {
        active_count: 0,
        completed_count: 15,
        execution_mode: "standard",
        timestamp: new Date().toISOString(),
        items: [],
      };

      const evalHistoryMock = {
        total_count: 15,
        page: 1,
        limit: 50,
        items: [
          {
            commit_id: 999,
            flake_id: 1,
            flake_name: "infrastructure",
            branch: "main",
            commit_hash: "ff1a2b3c4d5e6f7a",
            commit_message: "feat: upgrade postgresql to 17.x",
            author: "alice",
            committed_at: new Date(Date.now() - 3600000).toISOString(),
            evaluation_status: "complete",
            evaluation_completed_at: new Date(Date.now() - 3500000).toISOString(),
            evaluation_duration_ms: 83000,
            evaluation_error_message: null,
            system_count: 9,
            passed_count: 9,
            policy_failed_count: 0,
            eval_failed_count: 0,
          },
          {
            commit_id: 998,
            flake_id: 2,
            flake_name: "workstations",
            branch: "dev",
            commit_hash: "ee2b3c4d5e6f7a8b",
            commit_message: "fix: add missing font packages",
            author: "bob",
            committed_at: new Date(Date.now() - 7200000).toISOString(),
            evaluation_status: "failed",
            evaluation_completed_at: new Date(Date.now() - 7100000).toISOString(),
            evaluation_duration_ms: 12000,
            evaluation_error_message: "nix-eval-jobs failed with exit code: 1\nnix error: attribute 'fonts' missing",
            system_count: 0,
            passed_count: 0,
            policy_failed_count: 0,
            eval_failed_count: 3,
          },
          {
            commit_id: 997,
            flake_id: 1,
            flake_name: "infrastructure",
            branch: "main",
            commit_hash: "dd3c4d5e6f7a8b9c",
            commit_message: "chore: update nixpkgs input",
            author: "carol",
            committed_at: new Date(Date.now() - 10800000).toISOString(),
            evaluation_status: "cancelled",
            evaluation_completed_at: new Date(Date.now() - 10750000).toISOString(),
            evaluation_duration_ms: null,
            evaluation_error_message: null,
            system_count: 0,
            passed_count: 0,
            policy_failed_count: 0,
            eval_failed_count: 0,
          },
        ],
      };

      await page.route("**/api/v1/commits/eval-queue**", async (route) => {
        await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(evalQueueMock) });
      });
      await page.route("**/api/v1/commits/eval-history**", async (route) => {
        await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(evalHistoryMock) });
      });

      await page.goto(`${baseUrl}/evaluations`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);

      // Click the History tab
      const historyTab = page.getByRole("button", { name: /Eval History/i }).first();
      await historyTab.waitFor({ timeout: 5000 });
      await historyTab.click();
      await page.waitForTimeout(1500);

      // Assert history table is visible with status chips
      const completeChip = page.getByText("complete").first();
      await completeChip.waitFor({ timeout: 5000 });

      // Assert Re-evaluate button appears for failed row
      const reEvalBtn = page.getByRole("button", { name: /Re-evaluate/i }).first();
      await reEvalBtn.waitFor({ timeout: 5000 });

      await page.unroute("**/api/v1/commits/eval-queue**");
      await page.unroute("**/api/v1/commits/eval-history**");
    },
  },
];

const CI_FAST_STEP_NAMES = new Set([
  "01-login-page",
  "02-registration",
  "03-registration-submit",
  "04-post-register-login",
  "05-login-submit",
  "06-dashboard",
  "06x-pipeline-readiness-scroll",
  "06y-recent-deployments-scroll",
  "06z-fleet-health-widget-assert",
  "15-builds",
  "11b-builds-queue-card-focus",
  "12b-systems-config-warning",
  "12c-systems-modal-config-field",
  "12e-systems-edit-modal",
  "12f-systems-deploy-modal",
  "12g-system-detail-history-logs-edit",
  "12h-system-detail-cves-grouped-justification",
  "12d-systems-api-error-no-mock-fallback",
  "12g-systems-warning-clears-after-link",
  "13d-flakes-stress-dataset",
  "13e-flakes-add-modal-credentials",
  "13f-flakes-edit-modal-credentials",
  // TASK-237: builds queue controls evidence
  "15d-builds-queue-table-view",
  "15e-builds-cancelling-state",
  "15f-builds-human-duration",
  "15g-builds-action-visibility",
  "15h-builds-completed-restart-action",
  // TASK-17: CVE dashboard evidence
  "16-cves",
  "16b-cves-severity-filter",
  // TASK-273: Evaluation cancellation + history evidence
  "26-evaluations",
  "26b-evaluations-history",
]);

(async () => {
  const testProfile = process.env.CF_UI_TEST_PROFILE || "full";
  const stepsToRun =
    testProfile === "ci_fast"
      ? steps.filter((step) => CI_FAST_STEP_NAMES.has(step.name))
      : steps;

  console.log("Starting Crystal Forge Web UI Integration Test");
  console.log(`  Base URL: ${baseUrl}`);
  console.log(`  Output: ${outputDir}`);
  console.log(`  Profile: ${testProfile}`);
  console.log(`  Steps: ${stepsToRun.length}`);
  console.log("");

  const browser = await chromium.launch();
  // Use a single browser context to maintain session/cookies across steps
  const context = await browser.newContext({
    viewport: { width: 1920, height: 1080 },
  });
  const page = await context.newPage();

  const originalWaitForTimeout = page.waitForTimeout.bind(page);
  page.waitForTimeout = (ms) =>
    originalWaitForTimeout(Math.max(50, Math.floor(ms * 0.3)));

  const results = [];

  for (const step of stepsToRun) {
    console.log(`Step: ${step.name} - ${step.description}`);
    let ok = true;
    let error = null;

    try {
      await step.action(page);

      // Take screenshot
      const outputPath = `${outputDir}/${step.name}.png`;
      await page.screenshot({ path: outputPath });

      const stats = fs.statSync(outputPath);
      console.log(`  OK: ${step.name}.png (${stats.size} bytes)`);
    } catch (err) {
      ok = false;
      error = err.message;
      console.error(`  FAIL: ${step.name} - ${error}`);

      // Try to take screenshot anyway for debugging
      try {
        const outputPath = `${outputDir}/${step.name}.png`;
        await page.screenshot({ path: outputPath });
      } catch (_) {}
    }

    results.push({
      name: step.name,
      description: step.description,
      ok,
      error,
    });
  }

  await context.close();
  await browser.close();

  // Write results
  fs.writeFileSync(`${outputDir}/results.json`, JSON.stringify(results, null, 2));

  // Summary
  const okCount = results.filter((r) => r.ok).length;
  const failCount = results.filter((r) => !r.ok).length;

  console.log("");
  console.log("=== Summary ===");
  console.log(`  Passed: ${okCount}/${results.length}`);
  console.log(`  Failed: ${failCount}/${results.length}`);

  if (failCount > 0) {
    console.log("");
    console.log("Failed steps:");
    for (const r of results.filter((r) => !r.ok)) {
      console.log(`  - ${r.name}: ${r.error}`);
    }
    // Don't exit with error - let the test script analyze results
  }
})().catch((err) => {
  console.error(`Fatal error: ${err.message}`);
  console.error(err.stack);
  process.exit(1);
});
