/**
 * Captures the real React and Dioxus drawers with identical queue identity.
 *
 * Run with the repository's Playwright/Chromium environment:
 * node capture-eval-drawer-shell.cjs <offline-design-dir> <output-dir>
 * CF_UI_DEV_BASE_URL and CF_UI_DEV_API_BASE_URL select an isolated preview.
 * Queue/log fixtures replace browser responses, not persisted evaluation rows.
 * The normal preview login can create an authentication session.
 * This first contract checks shell typography, not full drawer pixel parity.
 */
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const { chromium, expect } = require('@playwright/test');

async function settle(dialog) {
  await dialog.evaluate(async element => {
    await Promise.all(element.getAnimations().filter(animation =>
      animation.effect.getTiming().iterations !== Infinity
    ).map(animation => animation.finished));
    await document.fonts.ready;
  });
}

async function measurements(dialog) {
  return dialog.evaluate(element => {
    const parts = {
      header: '.fl-tray-head',
      metadata: '.fl-tray-head .mono',
      time: '.ed-stat-val',
      tab: '.sd-tab',
      log: '.fl-diff',
    };
    return Object.fromEntries(Object.entries(parts).map(([name, selector]) => {
      const node = element.querySelector(selector);
      if (!node) throw new Error(`Missing drawer part: ${name}`);
      const style = getComputedStyle(node);
      const rect = node.getBoundingClientRect();
      return [name, {
        bounds: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
        ...Object.fromEntries(['fontFamily', 'fontSize', 'fontWeight', 'lineHeight',
          'color', 'padding', 'gap'].map(property => [property, style[property]])),
      }];
    }));
  });
}

async function main() {
  const [designDirectory, outputDirectory] = process.argv.slice(2);
  if (!designDirectory || !outputDirectory) {
    throw new Error('Usage: node capture-eval-drawer-shell.cjs <offline-design-dir> <output-dir>');
  }
  const base = process.env.CF_UI_DEV_BASE_URL || 'http://127.0.0.1:8080';
  const api = process.env.CF_UI_DEV_API_BASE_URL || 'http://127.0.0.1:3445';
  fs.mkdirSync(outputDirectory, {recursive: true});
  const browser = await chromium.launch({args: ['--allow-file-access-from-files']});
  try {
    for (const [name, theme, width] of [['dark', 'dark', 1440], ['light', 'light', 1440], ['narrow-dark', 'dark', 720]]) {
      const context = await browser.newContext({viewport: {width, height: 1000}, deviceScaleFactor: 1});
      try {
        const design = await context.newPage();
        design.on('pageerror', error => console.error('Reference render:', error.message));
        await design.goto(`file://${path.resolve(designDirectory, 'crystal-forge.html')}`);
        await design.locator('.nav-item').filter({hasText: 'Evaluations'}).click();
        // Mount the unchanged reference component with a numeric identity that
        // both applications can represent. Keep its status and counts intact.
        const fixture = await design.evaluate(theme => {
          document.documentElement.setAttribute('data-theme', theme);
          const ev = {...ACTIVE_EVALS[0], id: '488', logLines: () => [], meta: EVAL_STATUS_META[ACTIVE_EVALS[0].status]};
          const host = document.createElement('div');
          document.body.appendChild(host);
          ReactDOM.createRoot(host).render(React.createElement(EvalDrawer, {
            ev, onClose: () => {}, onCancel: () => {},
          }));
          return JSON.parse(JSON.stringify(ev));
        }, theme);
        const reference = design.getByRole('dialog', {name: 'Evaluation detail', exact: true});
        await expect(reference).toBeVisible();
        await settle(reference);

        const login = await context.request.post(`${api}/api/auth/local/login`, {
          data: {username: process.env.CF_UI_TEST_USERNAME || 'admin', password: process.env.CF_UI_TEST_PASSWORD || 'password'},
        });
        assert.equal(login.status(), 200, 'Isolated preview login');
        const actual = await context.newPage();
        await actual.addInitScript(({api, theme}) => {
          localStorage.setItem('cf_backend_origin', api);
          localStorage.setItem('theme', theme);
        }, {api, theme});
        const now = new Date().toISOString();
        const item = {
          commit_id: 488, flake_id: 488, flake_name: fixture.flake,
          branch: fixture.branch, commit_hash: fixture.commit,
          commit_message: null, author: null, committed_at: now, enqueued_at: now,
          is_latest_per_flake: true, evaluation_status: fixture.status,
          queue_position: 1, systems: [], system_count: fixture.systemCount,
          passed_count: fixture.policyPass, policy_failed_count: fixture.policyFail,
          eval_failed_count: 0, attempt_number: 1,
        };
        await actual.route('**/commits/eval-queue?*', route => route.fulfill({json: {
          active_count: 1, completed_count: 0, successful_count: 0, failed_count: 0,
          domain_total: 1, filtered_total: 1, execution_mode: 'mock', items: [item], timestamp: now,
        }}));
        await actual.route('**/commits/488/eval/logs?*', route => route.fulfill({json: []}));
        await actual.goto(`${base}/evaluations`);
        const coach = actual.getByRole('complementary', {name: 'Crystal Forge Coach', exact: true});
        // The saved Coach preference arrives asynchronously. It can disappear
        // between the visibility check and click; retry the normal interaction.
        await expect(async () => {
          if (await coach.isVisible()) {
            await coach.getByRole('button', {name: 'Close Crystal Forge Coach', exact: true}).click({timeout: 1000});
          }
          await actual.getByRole('row').filter({hasText: fixture.commit}).click({timeout: 1000});
        }).toPass({timeout: 15000});
        const production = actual.getByRole('dialog', {name: 'Evaluation detail', exact: true});
        await expect(production).toBeVisible();
        await actual.evaluate(theme => document.documentElement.setAttribute('data-theme', theme), theme);
        await settle(production);
        const report = {reference: await measurements(reference), production: await measurements(production)};
        fs.writeFileSync(path.join(outputDirectory, `${name}.json`), JSON.stringify(report, null, 2));
        await reference.screenshot({path: path.join(outputDirectory, `${name}.design.png`), animations: 'disabled'});
        await production.screenshot({path: path.join(outputDirectory, `${name}.production.png`), animations: 'disabled'});
        for (const part of ['metadata', 'time', 'tab', 'log']) {
          for (const property of ['fontSize', 'fontWeight', 'fontFamily', 'lineHeight']) {
            assert.equal(report.production[part][property], report.reference[part][property], `${theme}: ${part}.${property}`);
          }
        }
        assert.equal(report.production.metadata.color, report.reference.metadata.color, `${name}: metadata.color`);
        assert.equal(report.production.header.bounds.height, report.reference.header.bounds.height, `${name}: header.height`);
        assert.equal(report.production.tab.bounds.height, report.reference.tab.bounds.height, `${name}: tab.height`);
        console.log(`PASS ${name}: shell typography and header/tab heights; screenshots and measurements saved (not full pixel parity)`);
      } finally { await context.close(); }
    }
  } finally { await browser.close(); }
}

main().catch(error => {console.error(error); process.exitCode = 1;});
