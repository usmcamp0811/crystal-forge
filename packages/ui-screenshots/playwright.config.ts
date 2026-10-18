import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: ".",
  testMatch: "seed-db.spec.ts",
  timeout: 120_000,
  fullyParallel: false,
  retries: 0,
  workers: 1,
  reporter: "list",
  use: {
    headless: false,
    viewport: { width: 1440, height: 900 },
    actionTimeout: 15_000,
    screenshot: "only-on-failure",
    trace: "on-first-retry",
  },
});
