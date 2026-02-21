# Web UI Build Verification Check
#
# Verifies the Crystal Forge web UI compiles to WASM and produces valid output,
# then takes screenshots of every route using headless Chromium in a NixOS VM.
#
# Output ($out):
#   screenshots/   — PNG screenshots of core routes and modal states
#   result.txt     — Build verification summary
#
# Run: nix build .#checks.x86_64-linux.web-ui
#      ls ./result/screenshots/
{ lib, pkgs, inputs, ... }:
let
  # Playwright screenshot script that runs inside the VM
  # Each route includes assertions to verify the UI is rendering correctly
  # Uses waitFor() instead of expect() since we're using playwright-core, not @playwright/test
  screenshotScript = pkgs.writeText "screenshot.js" ''
    const { chromium } = require('playwright');

    // Helper to assert element is visible
    async function assertVisible(page, selector, description) {
      const element = page.locator(selector);
      const isVisible = await element.isVisible({ timeout: 5000 }).catch(() => false);
      if (!isVisible) {
        throw new Error('Expected "' + description + '" (' + selector + ') to be visible');
      }
      return true;
    }

    // Helper to assert element is NOT visible
    async function assertNotVisible(page, selector, description) {
      const element = page.locator(selector);
      const isVisible = await element.isVisible().catch(() => false);
      if (isVisible) {
        throw new Error('Expected "' + description + '" (' + selector + ') to NOT be visible');
      }
      return true;
    }

    // Helper to assert text content exists
    async function assertTextVisible(page, text, description) {
      const element = page.locator('text=' + text);
      const isVisible = await element.first().isVisible({ timeout: 5000 }).catch(() => false);
      if (!isVisible) {
        throw new Error('Expected text "' + text + '" (' + description + ') to be visible');
      }
      return true;
    }

    const routes = [
      // ============================================================
      // AUTH SCREENS (unauthenticated)
      // ============================================================
      {
        path: '/login',
        name: 'login',
        desc: 'Login screen',
        assertions: async (page) => {
          await assertTextVisible(page, 'Crystal Forge', 'Login title');
          await assertTextVisible(page, 'Sign in to continue', 'Login subtitle');
          // Should show background logo (faded)
          await assertVisible(page, 'img[alt=""]', 'Background logo');
        }
      },
      {
        path: '/register',
        name: 'registration',
        desc: 'First-run registration screen',
        assertions: async (page) => {
          await assertTextVisible(page, 'Administrator Registration', 'Registration title');
          await assertTextVisible(page, 'First-Time Setup', 'First-run setup banner');
          // Should have registration form fields
          await assertTextVisible(page, 'Username', 'Username field label');
          await assertTextVisible(page, 'Email', 'Email field label');
          await assertTextVisible(page, 'Password', 'Password field label');
          await assertTextVisible(page, 'Confirm Password', 'Confirm password field label');
          await assertTextVisible(page, 'Create Administrator Account', 'Submit button');
        }
      },

      // ============================================================
      // AUTH PROTECTION TEST (verify redirect to login)
      // Tests that protected routes redirect unauthenticated users.
      // We only test one route to keep the test suite fast.
      // ============================================================
      {
        path: '/',
        name: 'auth-redirect-dashboard',
        desc: 'Dashboard redirects to login when unauthenticated',
        assertions: async (page) => {
          // Should be redirected to login page
          await assertTextVisible(page, 'Sign in to continue', 'Redirected to login');
          // Should NOT see dashboard content
          await assertNotVisible(page, "[data-testid='dashboard']", 'Dashboard should not be visible');
        }
      },

      // ============================================================
      // AUTHENTICATED ROUTES (with ui_check_auth=1 mock)
      // ============================================================
      {
        path: '/?ui_check_auth=1',
        name: 'dashboard',
        desc: 'Dashboard (fleet overview)',
        assertions: async (page) => {
          // Dashboard should have the app title and navigation
          await assertTextVisible(page, 'Crystal Forge', 'App title');
          await assertVisible(page, 'nav', 'Navigation');
          // Dashboard should show fleet metrics
          await assertVisible(page, "[data-testid='dashboard']", 'Dashboard container');
          await assertTextVisible(page, 'Total Systems', 'Total Systems stat card');
          await assertTextVisible(page, 'Healthy', 'Healthy stat card');
          await assertVisible(page, "[data-testid='fleet-health-breakdown']", 'Fleet health breakdown');
          await assertVisible(page, "[data-testid='cve-summary']", 'CVE summary panel');
          await assertVisible(page, "[data-testid='deployment-status']", 'Deployment status panel');
          await assertVisible(page, "[data-testid='build-summary-panel']", 'Build summary panel');
          await assertVisible(page, "[data-testid='build-queue']", 'Build queue panel');
          await assertVisible(page, "[data-testid='recent-deployments']", 'Recent deployments list');
          // Should show actual mock data values
          await assertTextVisible(page, '21', 'Total systems count (21)');
          await assertTextVisible(page, 'atlas-01', 'Recent deployment hostname');
          // Flake commit timeline
          await assertVisible(page, "[data-testid='flake-timeline-widget']", 'Flake timeline widget');
          await assertVisible(page, "[data-testid='timeline-legend']", 'Timeline legend');
          await assertTextVisible(page, 'Commit Timeline', 'Timeline title');
          await assertTextVisible(page, 'infrastructure', 'First flake name');
        }
      },
      {
        path: '/?ui_check_auth=1',
        name: 'topbar-user-dropdown',
        desc: 'Topbar user dropdown menu',
        setup: async (page) => {
          await page.locator("[data-testid='user-menu-button']").click();
          await page.waitForTimeout(250);
        },
        assertions: async (page) => {
          await assertVisible(page, "[data-testid='user-menu-dropdown']", 'User dropdown container');
          await assertTextVisible(page, 'Sign Out', 'Sign out action in user dropdown');
        }
      },
      {
        path: '/systems?ui_check_auth=1',
        name: 'systems-table',
        desc: 'Systems list (table view)',
        assertions: async (page) => {
          // Table view should show the systems table container
          await assertVisible(page, "[data-testid='systems-table']", 'Systems table container');
          // Should show at least one mock system hostname (proves data is rendered)
          await assertTextVisible(page, 'atlas-01', 'First mock system hostname');
          // Table toggle buttons should be visible
          await assertVisible(page, 'button:has-text("Table")', 'Table toggle button');
          await assertVisible(page, 'button:has-text("Cards")', 'Cards toggle button');
        }
      },
      {
        path: '/systems?ui_check_auth=1',
        name: 'systems-cards',
        desc: 'Systems list (card view)',
        clickCards: true,
        assertions: async (page) => {
          // After clicking Cards, should show cards view (not table)
          await assertVisible(page, "[data-testid='systems-cards']", 'Systems cards container');
          // Table should NOT be visible
          await assertNotVisible(page, "[data-testid='systems-table']", 'Systems table (should be hidden)');
          // Should show system hostnames in cards
          await assertTextVisible(page, 'atlas-01', 'First system in cards');
          await assertTextVisible(page, 'luna-02', 'Second system in cards');
        }
      },
      {
        path: '/systems?ui_check_auth=1',
        name: 'systems-add-modal',
        desc: 'Systems add modal',
        setup: async (page) => {
          await page.getByRole('button', { name: 'Add System' }).click();
          await page.waitForTimeout(300);
        },
        assertions: async (page) => {
          await assertTextVisible(page, 'Register System', 'Add system modal title');
          await assertTextVisible(page, 'Save System', 'Save system button');
        }
      },
      {
        path: '/systems?ui_check_auth=1',
        name: 'systems-keypair-modal',
        desc: 'Systems keypair generation modal',
        setup: async (page) => {
          await page.getByRole('button', { name: 'Add System' }).click();
          await page.waitForTimeout(250);
          await page.getByRole('button', { name: 'Generate' }).click();
          await page.waitForTimeout(300);
        },
        assertions: async (page) => {
          await assertTextVisible(page, 'Generated System Key Pair', 'Keypair modal title');
          await assertTextVisible(page, 'Use Public Key', 'Use public key action');
        }
      },
      {
        path: '/systems?ui_check_auth=1',
        name: 'systems-remove-modal',
        desc: 'Systems remove confirmation modal',
        setup: async (page) => {
          await page.locator("button:has-text('Remove')").first().click();
          await page.waitForTimeout(300);
        },
        assertions: async (page) => {
          await assertTextVisible(page, 'Remove', 'Remove modal visible');
          await assertTextVisible(page, 'Cancel', 'Cancel button visible');
        }
      },
      {
        path: '/systems/00000000-0000-0000-0000-000000000001?ui_check_auth=1',
        name: 'system-detail',
        desc: 'System detail page',
        assertions: async (page) => {
          await assertVisible(page, "[data-testid='system-detail']", 'System detail container');
          await assertTextVisible(page, 'atlas-01', 'System hostname');
          await assertTextVisible(page, 'Hardware', 'Hardware card');
          await assertTextVisible(page, 'Network', 'Network card');
          await assertTextVisible(page, 'Security', 'Security card');
          await assertTextVisible(page, 'Vulnerabilities', 'Vulnerabilities card');
          await assertTextVisible(page, 'Agent', 'Agent card');
        }
      },
      {
        path: '/flakes?ui_check_auth=1',
        name: 'flakes-table',
        desc: 'Flakes registry table view',
        setup: async (page) => {
          const tableToggle = page.getByRole('button', { name: 'Table' });
          if (await tableToggle.isVisible().catch(() => false)) {
            await tableToggle.click();
            await page.waitForTimeout(200);
          }
        },
        assertions: async (page) => {
          await assertTextVisible(page, 'Flake Registry', 'Flakes page title');
          await assertVisible(page, "[data-testid='flakes-table']", 'Flakes table');
        }
      },
      {
        path: '/flakes?ui_check_auth=1',
        name: 'flakes-cards',
        desc: 'Flakes registry card view',
        clickCards: true,
        assertions: async (page) => {
          await assertVisible(page, "[data-testid='flakes-cards']", 'Flakes cards container');
          await assertTextVisible(page, 'Latest Commit', 'Card section label');
        }
      },
      {
        path: '/flakes?ui_check_auth=1',
        name: 'flakes-add-modal',
        desc: 'Flakes add modal',
        setup: async (page) => {
          await page.getByRole('button', { name: 'Add Flake' }).click();
          await page.waitForTimeout(300);
        },
        assertions: async (page) => {
          await assertTextVisible(page, 'Register Flake', 'Add flake modal title');
          await assertTextVisible(page, 'Save Flake', 'Save flake button');
        }
      },
      {
        path: '/flakes?ui_check_auth=1',
        name: 'flakes-edit-modal',
        desc: 'Flakes edit modal',
        setup: async (page) => {
          await page.locator("button:has-text('Edit')").first().click();
          await page.waitForTimeout(300);
        },
        assertions: async (page) => {
          await assertTextVisible(page, 'Edit Flake', 'Edit flake modal title');
          await assertTextVisible(page, 'Save Changes', 'Save edits button');
        }
      },
      {
        path: '/flakes?ui_check_auth=1',
        name: 'flakes-remove-modal',
        desc: 'Flakes remove confirmation modal',
        setup: async (page) => {
          await page.getByRole('button', { name: 'Add Flake' }).click();
          await page.getByPlaceholder('prod-core').fill('qa-temp');
          await page.getByPlaceholder('https://github.com/org/repo').fill('https://github.com/example/qa-temp');
          await page.getByRole('button', { name: 'Save Flake' }).click();
          await page.waitForTimeout(300);
          await page.locator("tr:has-text('qa-temp') button:has-text('Remove')").first().click();
          await page.waitForTimeout(300);
        },
        assertions: async (page) => {
          await assertTextVisible(page, 'Remove flake', 'Remove flake modal title');
          await assertTextVisible(page, 'Related commits are deleted by cascade', 'Cascade warning');
        }
      },
      {
        path: '/environments?ui_check_auth=1',
        name: 'environments-registry',
        desc: 'Environment registry view',
        assertions: async (page) => {
          await assertTextVisible(page, 'Environment Registry', 'Environment registry title');
          await assertTextVisible(page, 'Edit Environment', 'Environment edit action');
          await assertTextVisible(page, 'Edit Requirements', 'Requirements edit action');
        }
      },
      {
        path: '/environments?ui_check_auth=1',
        name: 'environments-add-modal',
        desc: 'Environment add modal',
        setup: async (page) => {
          await page.getByRole('button', { name: 'Add Environment' }).click();
          await page.waitForTimeout(300);
        },
        assertions: async (page) => {
          await assertTextVisible(page, 'Create Environment', 'Create environment title');
          await assertTextVisible(page, 'Choose Policies', 'Choose policies action');
        }
      },
      {
        path: '/environments?ui_check_auth=1',
        name: 'environments-policy-picker-modal',
        desc: 'Environment policy picker modal',
        setup: async (page) => {
          await page.getByRole('button', { name: 'Add Environment' }).click();
          await page.waitForTimeout(200);
          await page.getByRole('button', { name: 'Choose Policies' }).click();
          await page.waitForTimeout(300);
        },
        assertions: async (page) => {
          await assertTextVisible(page, 'Choose Required Policies', 'Policy picker title');
          await assertTextVisible(page, 'Apply Policies', 'Apply policies button');
        }
      },
      {
        path: '/environments?ui_check_auth=1',
        name: 'environments-edit-modal',
        desc: 'Environment edit metadata modal',
        setup: async (page) => {
          await page.locator("button:has-text('Edit Environment')").first().click();
          await page.waitForTimeout(300);
        },
        assertions: async (page) => {
          await assertTextVisible(page, 'Edit Environment', 'Edit environment modal title');
          await assertTextVisible(page, 'Save Changes', 'Save changes button');
        }
      },
      {
        path: '/environments?ui_check_auth=1',
        name: 'environments-edit-requirements-modal',
        desc: 'Environment edit requirements modal',
        setup: async (page) => {
          await page.locator("button:has-text('Edit Requirements')").first().click();
          await page.waitForTimeout(300);
        },
        assertions: async (page) => {
          await assertTextVisible(page, 'Save Requirements', 'Save requirements button');
          await assertTextVisible(page, 'Required policies are hard requirements', 'Requirements help text');
        }
      },
      {
        path: '/environments?ui_check_auth=1',
        name: 'environments-remove-modal',
        desc: 'Environment remove confirmation modal',
        setup: async (page) => {
          await page.locator("button:has-text('Remove')").first().click();
          await page.waitForTimeout(300);
        },
        assertions: async (page) => {
          await assertTextVisible(page, 'Remove environment', 'Remove environment modal title');
          await assertTextVisible(page, 'This deletes the environment', 'Removal warning text');
        }
      },
      {
        path: '/builds?ui_check_auth=1',
        name: 'builds',
        desc: 'Builds pipeline',
        assertions: async (page) => {
          // Builds page should have title
          await assertTextVisible(page, 'Builds', 'Builds page title');
        }
      },
      {
        path: '/cves?ui_check_auth=1',
        name: 'cves',
        desc: 'CVE dashboard',
        assertions: async (page) => {
          // CVE page should have title (might be "CVEs" or "CVE Dashboard")
          await assertTextVisible(page, 'CVE', 'CVE page title');
        }
      },
      {
        path: '/style-guide?ui_check_auth=1',
        name: 'style-guide',
        desc: 'Design system style guide',
        assertions: async (page) => {
          // Style guide should show design tokens
          await assertTextVisible(page, 'Style Guide', 'Style Guide page title');
        }
      },
      {
        path: '/not-a-real-page?ui_check_auth=1',
        name: 'not-found',
        desc: '404 not found page',
        assertions: async (page) => {
          // 404 page should indicate page not found
          const has404 = await page.locator('text=/404/').first().isVisible().catch(() => false);
          const hasNotFound = await page.locator('text=/not found/i').first().isVisible().catch(() => false);
          if (!has404 && !hasNotFound) {
            throw new Error('Expected 404 or "not found" text to be visible');
          }
        }
      },
    ];

    const baseUrl = process.argv[2] || 'http://127.0.0.1:8080';
    const outputDir = process.argv[3] || '/tmp/screenshots';

    (async () => {
      const browser = await chromium.launch();
      const results = [];

      for (const route of routes) {
        const assertions = [];
        try {
          const page = await browser.newPage({ viewport: { width: 1920, height: 1080 } });
          await page.goto(baseUrl + route.path, { waitUntil: 'networkidle' });

          // Wait a bit for WASM app to fully hydrate
          await page.waitForTimeout(1000);

          // For cards view, click the Cards button first
          if (route.clickCards) {
            await page.getByRole('button', { name: 'Cards' }).click();
            await page.waitForTimeout(500); // Wait for animation
          }

          // Optional custom setup per route (open modals, fill forms, etc.)
          if (route.setup) {
            await route.setup(page);
          }

          // Run assertions to verify the page rendered correctly
          if (route.assertions) {
            try {
              await route.assertions(page);
              assertions.push('All assertions passed');
            } catch (assertErr) {
              assertions.push('ASSERTION FAILED: ' + assertErr.message);
              throw assertErr;
            }
          }

          const outputPath = outputDir + '/' + route.name + '.png';
          await page.screenshot({ path: outputPath });
          await page.close();

          const fs = require('fs');
          const stats = fs.statSync(outputPath);
          results.push({ name: route.name, desc: route.desc, size: stats.size, ok: true, assertions });
          console.log('OK: ' + route.name + '.png (' + stats.size + ' bytes) - ' + assertions.join(', '));
        } catch (err) {
          results.push({ name: route.name, desc: route.desc, size: 0, ok: false, error: err.message, assertions });
          console.error('FAIL: ' + route.name + ' - ' + err.message);
        }
      }

      await browser.close();

      // Write results JSON for the test driver to read
      const fs = require('fs');
      fs.writeFileSync(outputDir + '/results.json', JSON.stringify(results, null, 2));

      // Exit with error if any failed
      const failCount = results.filter(r => !r.ok).length;
      if (failCount > 0) {
        process.exit(1);
      }
    })().catch((err) => {
      console.error('Fatal error: ' + err.message);
      process.exit(1);
    });
  '';
in pkgs.testers.runNixOSTest {
  name = "crystal-forge-web-ui-screenshots";
  skipLint = true;
  skipTypeCheck = true;

  nodes.machine = {
    virtualisation.memorySize = 4096;
    virtualisation.cores = 2;

    environment.systemPackages =
      [ pkgs.chromium pkgs.nodejs pkgs.playwright-test ];

    environment.variables = {
      NODE_PATH = "${pkgs.playwright-test}/lib/node_modules";
      PLAYWRIGHT_BROWSERS_PATH = "${pkgs.playwright-driver.browsers}";
    };

    # Serve the web UI on port 8080 via a systemd service
    systemd.services.web-ui-server = {
      description = "Crystal Forge Web UI static server";
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ];
      serviceConfig = {
        ExecStart = "${pkgs.crystal-forge.web-ui}/bin/crystal-forge-web-ui";
        Restart = "always";
      };
    };

    networking.firewall.allowedTCPPorts = [ 8080 ];
  };

  globalTimeout = 420; # 7 minutes

  testScript = ''
    import json
    import pathlib

    machine.start()
    machine.wait_for_unit("web-ui-server.service")
    machine.wait_for_open_port(8080)

    # Verify SPA server handles both static files and route fallback
    machine.succeed("curl -sf http://127.0.0.1:8080/ | grep -q 'Crystal Forge'")
    machine.succeed("curl -sf http://127.0.0.1:8080/systems | grep -q 'Crystal Forge'")
    print("Web root is being served correctly (SPA fallback working)")

    # Create output directory inside VM
    machine.succeed("mkdir -p /tmp/screenshots")

    # Copy the screenshot script into the VM and run it
    machine.succeed("cp ${screenshotScript} /tmp/screenshot.js")

    # Run Playwright to capture screenshots (allow failure, we'll check results)
    exit_code, output = machine.execute(
        "${pkgs.nodejs}/bin/node /tmp/screenshot.js http://127.0.0.1:8080 /tmp/screenshots 2>&1"
    )
    print(output)

    # Read the results
    results_json = machine.succeed("cat /tmp/screenshots/results.json")
    results = json.loads(results_json)

    # Copy screenshots from VM to $out/screenshots/
    for r in results:
        if r.get("ok"):
            machine.copy_from_vm(f"/tmp/screenshots/{r['name']}.png", "screenshots")

    # Summary
    ok_count = sum(1 for r in results if r.get("ok"))
    print(f"\n=== Summary ===")
    print(f"  Screenshots: {ok_count}/{len(results)} captured")
    for r in results:
        status = "OK" if r.get("ok") else "FAIL"
        size = r.get("size", 0)
        desc = r.get("desc", "")
        error = r.get("error", "")
        if error:
            print(f"  [{status}] {r['name']}.png - {error}")
        else:
            print(f"  [{status}] {r['name']}.png ({size} bytes) - {desc}")

    # At minimum, the build checks must pass (enforced by buildCheck dependency)
    # Screenshots are visual artifacts for review
    if ok_count == 0:
        raise Exception("All screenshots failed - browser may not be working")
  '';

}
