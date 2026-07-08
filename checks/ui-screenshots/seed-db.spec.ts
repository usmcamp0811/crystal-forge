/**
 * seed-db.spec.ts — Seed the Crystal Forge database via UI interactions.
 *
 * This Playwright script drives the real UI to create environments, flakes,
 * systems, builders, deployment policies, users, and caches — exactly as a
 * human would. After this script completes, the database contains a
 * reproducible golden dataset that can be snapshotted and used for:
 *
 *   1. Generating fixture JSON for the design example
 *   2. Running deterministic UI tests
 *   3. Validating that the UI behaviour matches the design spec
 *
 * Prerequisites:
 *   - `run-ui-dev` must be running (PostgreSQL + CF API server + Dioxus dev server)
 *   - API: http://127.0.0.1:3445
 *   - UI:  http://localhost:8080
 *
 * Usage:
 *   cd checks/ui-screenshots
 *   npx playwright test seed-db.spec.ts --headed
 *
 * This script uses a single test with sequential steps so that login state
 * persists through the entire seeding process.
 */

import { test, expect, Page } from "@playwright/test";

// ---------------------------------------------------------------------------
// Test data — mirrors docs/design/CrystalForge/fixtures/crystal-forge.fixtures.json
// The order of creation matters: environments → flakes → systems → rest.
// ---------------------------------------------------------------------------

interface Environment {
  name: string;
  description: string;
  color: string;
}

const ENVIRONMENTS: Environment[] = [
  { name: "production", description: "Production environment", color: "#dc2626" },
  { name: "staging", description: "Pre-production testing environment", color: "#d97706" },
  { name: "development", description: "Development servers", color: "#2563eb" },
  { name: "ci", description: "CI/CD build environment", color: "#7c3aed" },
  { name: "sandbox", description: "Experimental sandbox", color: "#059669" },
];

interface Flake {
  name: string;
  url: string;
  branch: string;
}

const FLAKES: Flake[] = [
  { name: "dotfiles", url: "https://gitlab.com/usmcamp0811/dotfiles", branch: "main" },
  { name: "nixos-config", url: "https://gitlab.com/usmcamp0811/nixos-config", branch: "main" },
  { name: "crystal-forge", url: "https://gitlab.com/crystal-forge/crystal-forge", branch: "main" },
];

interface Builder {
  name: string;
  maxCores: number;
  maxMemoryGb: number;
  maxJobs: number;
  environments: string[];
}

const BUILDERS: Builder[] = [
  { name: "builder-01", maxCores: 16, maxMemoryGb: 64, maxJobs: 4, environments: ["production", "staging"] },
  { name: "builder-02", maxCores: 8, maxMemoryGb: 32, maxJobs: 2, environments: ["development"] },
  { name: "builder-ci", maxCores: 32, maxMemoryGb: 128, maxJobs: 8, environments: ["ci"] },
];

interface System {
  hostname: string;
  environment: string;
  flake: string;
  configName?: string;
  policy: string;
}

const SYSTEMS: System[] = [
  { hostname: "web-01", environment: "production", flake: "dotfiles", policy: "auto_latest" },
  { hostname: "web-02", environment: "production", flake: "dotfiles", policy: "auto_latest" },
  { hostname: "db-01", environment: "production", flake: "nixos-config", policy: "manual" },
  { hostname: "staging-web-01", environment: "staging", flake: "dotfiles", policy: "auto_latest" },
  { hostname: "dev-box-01", environment: "development", flake: "nixos-config", policy: "manual" },
  { hostname: "ci-builder-01", environment: "ci", flake: "crystal-forge", policy: "manual" },
  { hostname: "sandbox-01", environment: "sandbox", flake: "dotfiles", policy: "pinned" },
];

interface Policy {
  name: string;
  description: string;
  body: string;
}

const POLICIES: Policy[] = [
  {
    name: "canary-deploy",
    description: "Canary deployment — roll out to 10% of fleet first",
    body: JSON.stringify({ policy: "canary", threshold: 0.1 }, null, 2),
  },
  {
    name: "require-signed-commits",
    description: "All deployments must use signed commits",
    body: JSON.stringify({ policy: "signing", required: true }, null, 2),
  },
  {
    name: "maintenance-window",
    description: "Only deploy during approved maintenance windows",
    body: JSON.stringify({ policy: "maint-window", hours: "02:00-05:00 UTC" }, null, 2),
  },
];

interface User {
  email: string;
  displayName: string;
  password: string;
  role: string;
  environments: string[];
}

const USERS: User[] = [
  { email: "alice@crystal-forge.local", displayName: "Alice Admin", password: "password123", role: "Admin", environments: [] },
  { email: "bob@crystal-forge.local", displayName: "Bob Operator", password: "password123", role: "Operator", environments: ["production", "staging"] },
  { email: "carol@crystal-forge.local", displayName: "Carol Viewer", password: "password123", role: "Viewer", environments: ["development"] },
  { email: "dave@crystal-forge.local", displayName: "Dave Deploy", password: "password123", role: "Operator", environments: ["ci"] },
];

interface Cache {
  name: string;
  type: string;
  url: string;
  environments: string[];
}

const CACHES: Cache[] = [
  { name: "production-cache", type: "S3-compatible", url: "https://s3.example.com/cache", environments: ["production"] },
  { name: "staging-cache", type: "S3-compatible", url: "https://s3.example.com/staging-cache", environments: ["staging"] },
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function generatePublicKey(): string {
  const chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  let result = "";
  for (let i = 0; i < 64; i++) {
    result += chars.charAt(Math.floor(Math.random() * chars.length));
  }
  return result + "=";
}

function formatCount(val: number): string {
  return String(val).padStart(3);
}

// ── Step functions (each uses the same page for persistent auth) ──────────

async function login(page: Page) {
  await page.goto("http://localhost:8080/login");
  await expect(page.getByText("Sign in to continue")).toBeVisible({ timeout: 15000 });
  await page.getByPlaceholder("Enter your username").fill("admin");
  await page.getByPlaceholder("Enter your password").fill("password");
  await page.getByRole("button", { name: "Sign In" }).click();
  await expect(page).toHaveURL(/\/$/, { timeout: 15000 });
  console.log("  ✅ Logged in as admin");
}

async function createEnvironment(page: Page, env: Environment) {
  await page.goto("http://localhost:8080/environments");
  await expect(page.getByText("Environments")).toBeVisible({ timeout: 10000 });
  await page.getByRole("button", { name: "Add environment" }).click();
  await expect(page.locator(".modal-head")).toBeVisible({ timeout: 5000 });

  // Fill name
  const nameInput = page.locator("label").filter({ hasText: "Name" }).locator("input");
  await nameInput.fill(env.name);

  // Click color swatch
  const swatches = page.locator(".modal-body button[title]");
  const count = await swatches.count();
  for (let i = 0; i < count; i++) {
    const style = await swatches.nth(i).getAttribute("style");
    if (style?.includes(env.color)) {
      await swatches.nth(i).click();
      break;
    }
  }

  // Fill description
  const descInput = page.locator("label").filter({ hasText: "Description" }).locator("input");
  await descInput.fill(env.description);

  // Submit
  await page.getByRole("button", { name: "Add environment" }).last().click();
  await page.waitForTimeout(1000);
  console.log(`  ✅ Environment "${env.name}" created`);
}

async function createFlake(page: Page, flake: Flake) {
  await page.goto("http://localhost:8080/flakes");
  await expect(page.getByText("Flakes")).toBeVisible({ timeout: 10000 });
  await page.getByRole("button", { name: "Add flake" }).click();
  await expect(page.getByText("Add Flake")).toBeVisible({ timeout: 5000 });

  const labels = page.locator("label");
  await labels.filter({ hasText: "Flake Name" }).locator("input, select").first().fill(flake.name);
  await labels.filter({ hasText: "Repository URL" }).locator("input").fill(flake.url);
  await labels.filter({ hasText: "Branch" }).locator("input, select").first().fill(flake.branch);

  await page.getByRole("button", { name: /Add|Save|Create/ }).last().click();
  await page.waitForTimeout(1000);
  console.log(`  ✅ Flake "${flake.name}" created`);
}

async function createPolicy(page: Page, policy: Policy) {
  await page.goto("http://localhost:8080/deployment-policies");
  await expect(page.getByText("Deployment Policies")).toBeVisible({ timeout: 10000 });
  await page.getByRole("button", { name: "New custom policy" }).click();
  await expect(page.getByText("Policy Editor")).toBeVisible({ timeout: 5000 });

  await page.locator("label").filter({ hasText: /Policy Name|Name/ }).locator("input, textarea").fill(policy.name);
  await page.locator("label").filter({ hasText: "Description" }).locator("input, textarea").fill(policy.description);
  await page.locator("textarea").first().fill(policy.body);

  await page.getByRole("button", { name: /Save|Add|Create/ }).last().click();
  await page.waitForTimeout(1000);
  console.log(`  ✅ Policy "${policy.name}" created`);
}

async function createBuilder(page: Page, builder: Builder) {
  await page.goto("http://localhost:8080/builders");
  await expect(page.getByText("Builders")).toBeVisible({ timeout: 10000 });
  await page.getByRole("button", { name: "Register builder" }).click();
  await expect(page.getByText("Register Builder")).toBeVisible({ timeout: 5000 });

  await page.locator("label").filter({ hasText: /Name/ }).locator("input").fill(builder.name);

  // Fill public key if the field exists
  const keyInput = page.locator("label").filter({ hasText: /Public Key/ }).locator("input");
  if (await keyInput.isVisible().catch(() => false)) {
    await keyInput.fill(generatePublicKey());
  }

  // Fill number inputs for cores, memory, jobs
  const numInputs = page.locator('input[type="number"]');
  const numCount = await numInputs.count();
  if (numCount > 0) await numInputs.nth(0).fill(String(builder.maxCores));
  if (numCount > 1) await numInputs.nth(1).fill(String(builder.maxMemoryGb));
  if (numCount > 2) await numInputs.nth(2).fill(String(builder.maxJobs));

  await page.getByRole("button", { name: /Register|Save|Add/ }).last().click();
  await page.waitForTimeout(1000);
  console.log(`  ✅ Builder "${builder.name}" created`);
}

async function createSystem(page: Page, system: System) {
  await page.goto("http://localhost:8080/systems");
  await expect(page.getByText("Systems")).toBeVisible({ timeout: 10000 });
  await page.getByRole("button", { name: "Add system" }).click();
  await expect(page.getByText("Register System")).toBeVisible({ timeout: 5000 });

  await page.locator("label").filter({ hasText: "Hostname" }).locator("input").fill(system.hostname);
  await page.locator("label").filter({ hasText: "Public Key" }).locator("input").fill(generatePublicKey());

  // Select environment and flake from dropdowns
  await page.locator("label").filter({ hasText: "Environment" }).locator("select").selectOption(system.environment);
  await page.locator("label").filter({ hasText: "Flake Name" }).locator("select").selectOption(system.flake);

  if (system.configName) {
    await page.locator("label").filter({ hasText: "Flake Config Name" }).locator("input").fill(system.configName);
  }

  await page.locator("label").filter({ hasText: "Deployment Policy" }).locator("select").selectOption(system.policy);
  await page.getByRole("button", { name: /Save System|Register System/ }).click();
  await page.waitForTimeout(1000);
  console.log(`  ✅ System "${system.hostname}" created`);
}

async function createUser(page: Page, user: User) {
  await page.goto("http://localhost:8080/admin");
  await expect(page.getByText("Server Management")).toBeVisible({ timeout: 10000 });
  await page.getByRole("button", { name: "Add user" }).click();
  await expect(page.getByText("Create User")).toBeVisible({ timeout: 5000 });

  await page.locator("label").filter({ hasText: /Email/ }).locator("input").fill(user.email);
  await page.locator("label").filter({ hasText: /Display Name|Name/ }).locator("input").fill(user.displayName);
  await page.locator("label").filter({ hasText: /Password/ }).locator("input").fill(user.password);
  await page.locator("label").filter({ hasText: /Role/ }).locator("select").selectOption(user.role);

  await page.getByRole("button", { name: /Create|Save|Add/ }).last().click();
  await page.waitForTimeout(1000);
  console.log(`  ✅ User "${user.displayName}" created`);
}

async function createCache(page: Page, cache: Cache) {
  await page.goto("http://localhost:8080/caches");
  await expect(page.getByText("Cache Management")).toBeVisible({ timeout: 10000 });
  await page.getByRole("button", { name: "Add cache" }).click();
  await expect(page.getByText(/Add Cache|Cache/)).toBeVisible({ timeout: 5000 });

  await page.locator("label").filter({ hasText: /Name/ }).locator("input").fill(cache.name);
  await page.locator("label").filter({ hasText: /URL/ }).locator("input").fill(cache.url);

  const typeSelect = page.locator("label").filter({ hasText: /Type/ }).locator("select");
  if (await typeSelect.isVisible().catch(() => false)) {
    await typeSelect.selectOption(cache.type);
  }

  await page.getByRole("button", { name: /Save|Add|Create/ }).last().click();
  await page.waitForTimeout(1000);
  console.log(`  ✅ Cache "${cache.name}" created`);
}

function printSummary() {
  const totalEnvs = ENVIRONMENTS.length;
  const totalFlakes = FLAKES.length;
  const totalPolicies = POLICIES.length;
  const totalBuilders = BUILDERS.length;
  const totalSystems = SYSTEMS.length;
  const totalUsers = USERS.length;
  const totalCaches = CACHES.length;
  const total = totalEnvs + totalFlakes + totalPolicies + totalBuilders + totalSystems + totalUsers + totalCaches;

  console.log("");
  console.log("═══════════════════════════════════════════════");
  console.log("  DB Seeding Summary");
  console.log("───────────────────────────────────────────────");
  console.log(`  Environments:         ${formatCount(totalEnvs)}`);
  console.log(`  Flakes:               ${formatCount(totalFlakes)}`);
  console.log(`  Deployment Policies:  ${formatCount(totalPolicies)}`);
  console.log(`  Builders:             ${formatCount(totalBuilders)}`);
  console.log(`  Systems:              ${formatCount(totalSystems)}`);
  console.log(`  Users:                ${formatCount(totalUsers)}`);
  console.log(`  Caches:               ${formatCount(totalCaches)}`);
  console.log(`  ───────────────────────────────────────────`);
  console.log(`  Total entities:       ${formatCount(total)}`);
  console.log("═══════════════════════════════════════════════");
  console.log("");
  console.log("✅ DB seeding complete! You can now take a DB snapshot:");
  console.log("   pg_dump postgresql://crystal_forge@127.0.0.1:3042/crystal_forge > golden-dataset.sql");
  console.log("");
}

// ── Single sequential test ────────────────────────────────────────────────

test("Seed database with golden dataset via UI", async ({ page }) => {
  console.log("");
  console.log("🚀 Starting DB seeding via UI...");
  console.log("");

  // 1. Login
  await login(page);

  // 2. Environments
  console.log("\n📁 Creating environments...");
  for (const env of ENVIRONMENTS) {
    await createEnvironment(page, env);
  }

  // 3. Flakes
  console.log("\n❄️  Creating flakes...");
  for (const flake of FLAKES) {
    await createFlake(page, flake);
  }

  // 4. Deployment Policies
  console.log("\n📋 Creating deployment policies...");
  for (const policy of POLICIES) {
    await createPolicy(page, policy);
  }

  // 5. Builders
  console.log("\n🏗️  Creating builders...");
  for (const builder of BUILDERS) {
    await createBuilder(page, builder);
  }

  // 6. Systems (requires environments + flakes to exist)
  console.log("\n🖥️  Creating systems...");
  for (const system of SYSTEMS) {
    await createSystem(page, system);
  }

  // 7. Users
  console.log("\n👤 Creating users...");
  for (const user of USERS) {
    await createUser(page, user);
  }

  // 8. Caches
  console.log("\n💾 Creating caches...");
  for (const cache of CACHES) {
    await createCache(page, cache);
  }

  // 9. Summary
  console.log("");
  printSummary();
});
