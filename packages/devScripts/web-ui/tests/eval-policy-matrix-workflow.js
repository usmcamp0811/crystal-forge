/**
 * Exercises structured matrix presentation using a deterministic API fixture.
 * The backend's actual resolver/capture path has separate migration-backed tests.
 * These responses are browser-local; they never rewrite persisted evaluations.
 */
const { expect } = require('@playwright/test');
const { assertDialogKeyboard, dismissDialog } = require('./eval-dialog-workflow.js');

function id(index) {
  return `04880488-0000-4000-8000-${index.toString(16).padStart(12, '0')}`;
}

// Call explicitly when replacing the represented population. Expected UI names
// are asserted separately; this helper must not reconcile missing rows for it.
function inventoryFixture(rows, gates) {
  const configurations=[...new Set(rows.map(row=>row.configuration_name))].sort();
  gates={unavailable:0,...gates};
  expect(gates.passed+gates.failed+gates.evaluation_failed+gates.pending+gates.unavailable+gates.excluded).toBe(configurations.length);
  return {
    inventory:{execution_mode:'mock',configurations,selected_configurations:[...configurations]},
    configuration_summary:{total:configurations.length,...gates},
    configuration_failures:[],
  };
}

function denseMatrix(commitId) {
  const columns = Array.from({length: 128}, (_, i) => ({
    policy_version_id: id(i + 1), policy_id: id(i + 1001),
    name: `Fixture policy ${String(i).padStart(3, '0')}`, version: '1.0',
    identifiers: [`FIXTURE-${i}`], description: 'Captured fixture check description',
    assertion: null, attribute_path: null,
  }));
  const memberships = [
    Array.from({length: 96}, (_, i) => i),
    [...Array.from({length: 16}, (_, i) => i), ...Array.from({length: 16}, (_, i) => 96+i)],
    [...Array.from({length: 16}, (_, i) => 16+i), ...Array.from({length: 16}, (_, i) => 112+i)],
    Array.from({length: 8}, (_, i) => i),
    Array.from({length: 8}, (_, i) => i+8),
    [],
  ];
  const bundles = memberships.map((_, i) => ({
    bundle_version_id: id(2001+i), bundle_id: id(3001+i),
    name: `Fixture bundle ${i+1}`, version: 'v1', framework: `Fixture framework ${i+1}`,
    framework_version: '2026',
  }));
  const contexts = bundles.map((b, i) => ({
    bundle_version_id: b.bundle_version_id,
    assignment_id: id(4001+i), assignment_version_id: id(5001+i), source: 'assignment',
  }));
  const rows = Array.from({length: 60}, (_, r) => {
    const cells = columns.map((_, c) => {
      if (r === 2) return 'not_assigned';
      if (r === 3) return 'unknown';
      if (r === 0 && c === 0) return 'fail';
      if (r <= 1 && c === 16) return 'warn';
      if (r === 0 && c === 32) return 'unknown';
      return 'pass';
    });
    return {
      system_id: id(6001+r), system_name: `fixture-host-${String(r).padStart(2,'0')}`,
      configuration_name: `fixture-config-${r}`, resolution: r===3 ? 'unavailable' : 'resolved',
      evaluation_digest: r===3 ? null : 'fixture-only', assigned_bundles: contexts,
      policies: r===2 || r===3 ? [] : columns.map((c, i) => ({
        policy_version_id:c.policy_version_id, report_only:i===16, executable:true,
        memberships:contexts.filter((_, b)=>memberships[b].includes(i)),
      })), cells,
      details:cells.map((status, i)=>status==='fail' ? '[FIXTURE] services.openssh.settings.PermitRootLogin must equal no' : status==='warn' ? '[FIXTURE] Audit logging is report-only' : null),
    };
  });
  return {
    commit_id:commitId, policies:['CF agent'], systems:[],
    matrix:{evaluation_attempt_id:id(9001), attempt_number:1, attempt_status:'in_progress', provenance:'captured',
      source_label:{name:'TASK-488 matrix fixture',branch:'main'},
      // Config 3 has neither resolved applicability nor a known gate result.
      // Config 2 has an authoritative empty policy set and a passing gate.
      ...inventoryFixture(rows,{passed:58,failed:1,evaluation_failed:0,pending:0,unavailable:1,excluded:0}),
      columns,bundles,empty_bundle_metadata_complete:true,rows},
  };
}

async function runEvalPolicyMatrixWorkflow(page, commitId, screenshot, onSnapshot, onHistoryPhase=()=>{}, captureEvidence) {
  const originalViewport=page.viewportSize();
  const fixture = denseMatrix(commitId);
  const originalDrawer=page.getByRole('dialog',{name:'Evaluation detail',exact:true});
  fixture.matrix.commit_sha=await originalDrawer.getAttribute('data-commit-sha');
  fixture.matrix.attempt_number=Number(await originalDrawer.getAttribute('data-attempt-number'));
  fixture.matrix.mock_evidence=true;
  const endpoint = `**/commits/${commitId}/eval/policy-matrix*`;
  const pinnedEndpoint = `**/commits/${commitId}/eval/attempts/${fixture.matrix.evaluation_attempt_id}/policy-matrix*`;
  // Before capture, the backend's legacy snapshot has no authoritative plan or
  // inventory. A live tab must initialize defaults from the first usable capture
  // for this same UUID, without treating the placeholder as an empty capture.
  let response={...fixture,matrix:{...fixture.matrix,provenance:'legacy_unavailable',
    empty_bundle_metadata_complete:false,columns:[],bundles:[],rows:[],
    inventory:null,configuration_summary:null}};
  let fail=false;
  let pending=null;
  let release=null;
  await page.route(endpoint, async route => {
    if(pending) await pending;
    if(fail) await route.fulfill({status:503,body:'Fixture matrix unavailable'});
    else {
      const delivered=response;
      await route.fulfill({json:delivered});
      await onSnapshot(delivered.matrix);
    }
  });
  const pinnedRequests=[];
  let pinnedResponse=null;
  await page.route(pinnedEndpoint, async route => { pinnedRequests.push(route.request().url()); await route.fulfill({json:pinnedResponse ?? response}); });
  try {
    const drawer=page.getByRole('dialog',{name:'Evaluation detail',exact:true});
    const assertHeaders=async(snapshot=response.matrix)=>{
      const stat=label=>drawer.locator('.ed-stat').filter({has:page.getByText(label,{exact:true})}).locator('.ed-stat-val');
      const summary=snapshot?.configuration_summary;
      await expect(stat('Systems')).toHaveText(snapshot?.inventory && summary ? String(summary.total) : /—\s*Discovering/,{timeout:12000});
      if(summary && snapshot.inventory) {
        const population=new Set([...snapshot.rows.map(row=>row.configuration_name),...snapshot.inventory.configurations]);
        expect(summary.total).toBe(population.size);
        expect(summary.passed+summary.failed+summary.evaluation_failed+summary.pending+summary.unavailable+summary.excluded).toBe(summary.total);
        await expect(stat('Policy').locator('.ed-policy-pass,.ed-policy-zero').first()).toHaveText(String(summary.passed),{timeout:12000});
        await expect(stat('Policy').locator('.ed-policy-fail,.ed-policy-zero').last()).toHaveText(String(summary.failed),{timeout:12000});
        const units=[];
        if(summary.pending) units.push(`${summary.pending} pending`);
        if(summary.unavailable) units.push(`${summary.unavailable} unavailable`);
        if(summary.evaluation_failed) units.push(`${summary.evaluation_failed} eval errors`);
        if(summary.excluded) units.push(`${summary.excluded} not selected`);
        if(!summary.passed && !summary.failed && !summary.pending && !summary.unavailable && !summary.excluded && !summary.evaluation_failed) units.push('No results');
        await expect(stat('Policy').locator('.ed-stat-unit')).toHaveText(units,{timeout:12000});
      } else {
        await expect(stat('Policy')).toHaveText(/—\s*Pending/,{timeout:12000});
      }
    };
    const capture=async name=>{await assertHeaders(); await screenshot(name);};
    await drawer.getByRole('button',{name:'Policy matrix',exact:true}).click();
    await expect(drawer.getByRole('status').filter({hasText:'Discovery pending'})).toBeVisible();
    await expect(drawer.getByRole('table')).toHaveCount(0);
    await assertHeaders();
    expect(response.matrix.evaluation_attempt_id).toBe(fixture.matrix.evaluation_attempt_id);
    expect(response.matrix.columns).toHaveLength(0);
    expect(response.matrix.provenance).toBe('legacy_unavailable');
    expect(response.matrix.empty_bundle_metadata_complete).toBe(false);
    response=fixture;
    await expect(drawer.getByRole('button',{name:/^Bundles\s+6$/})).toBeVisible({timeout:12000});
    await expect(drawer.getByRole('button',{name:/^Bundles\s+6$/})).toHaveClass('active');
    await drawer.getByRole('button',{name:/^Controls\s+128$/}).click();
    await expect(drawer.getByRole('button',{name:'Failing only',exact:true})).toHaveClass(/active-filter/);
    await drawer.getByRole('button',{name:/^Bundles\s+6$/}).click();
    await expect(drawer.getByText('128 controls · 32 shared across bundles',{exact:true})).toBeVisible();
    await expect(drawer.getByRole('button',{name:/^Clean\s+56$/})).toBeVisible();
    await expect(drawer.getByRole('button',{name:/^Unavailable \/ N\/A\s+2$/})).toBeVisible();
    await expect(drawer.getByRole('columnheader')).toHaveCount(8);
    await expect(drawer.getByRole('cell')).toHaveCount(60 * 8);
    const expectedNames=Array.from({length:60},(_,i)=>`fixture-host-${String(i).padStart(2,'0')}`);
    const assertNames=async names=>{
      await expect.poll(()=>drawer.locator('tbody tr.pm-row .pm-row-toggle').evaluateAll(nodes=>nodes.map(node=>node.getAttribute('aria-label'))))
        .toEqual([...names].sort());
    };
    await drawer.getByRole('button',{name:'Name',exact:true}).click();
    await assertNames(expectedNames);
    expect(fixture.matrix.inventory.configurations).toEqual(Array.from({length:60},(_,i)=>`fixture-config-${i}`).sort());
    expect(fixture.matrix.inventory.selected_configurations).toEqual(fixture.matrix.inventory.configurations);
    expect(fixture.matrix.configuration_summary).toEqual({total:60,passed:58,failed:1,evaluation_failed:0,pending:0,unavailable:1,excluded:0});
    const assertNoInventedPanels=async()=>{
      await expect(drawer.getByRole('note',{name:'Matrix scope'})).toHaveCount(0);
      await expect(drawer.getByText('Platform evidence',{exact:true})).toHaveCount(0);
      await expect(drawer.getByText(/^Assertion text unavailable/)).toHaveCount(0);
    };
    await assertNoInventedPanels();
    const tablePolicyCounts=page.locator('table [title="Known attempt-scoped results; coverage may be partial."]');
    await expect(tablePolicyCounts).toHaveCount(1,{timeout:12000});
    await expect(tablePolicyCounts).not.toContainText('known');
    await expect(drawer.locator('.pm-controls-line')).toHaveCount(2);
    await capture('eval-matrix-bundles');
    await drawer.getByRole('button',{name:'Expand',exact:true}).click();
    await expect.poll(async()=>Math.round((await drawer.boundingBox()).width)).toBe(Math.min(1360,page.viewportSize().width));
    expect((await drawer.locator('.pm-controls').boundingBox()).height).toBeLessThanOrEqual(55);
    await capture('eval-matrix-maximized');
    await drawer.getByRole('button',{name:'Restore',exact:true}).click();
    await expect.poll(async()=>Math.round((await drawer.boundingBox()).width)).toBe(Math.min(840,page.viewportSize().width));
    // Reference geometry uses native minimum widths, not fixed preferred widths.
    const geometry=()=>drawer.evaluate(el=>{
      const w=n=>n.getBoundingClientRect().width;
      const heads=[...el.querySelectorAll('thead th')];
      const row=el.querySelector('tbody tr.pm-row');
      return {system:w(heads[0]),health:w(heads[1]),
        bundles:Object.fromEntries(heads.filter(n=>n.classList.contains('pm-th-bundle')).map(n=>[n.querySelector('.pm-th-bundle-name').textContent,w(n)])),
        row:row.getBoundingClientRect().height,bar:w(row.querySelector('.pm-health-bar')),
        nameLines:[...el.querySelectorAll('.pm-th-bundle-name')].map(n=>Math.round(n.getBoundingClientRect().height))};
    });
    const dense=await geometry();
    expect(dense.system).toBeGreaterThanOrEqual(180);
    expect(dense.health).toBeGreaterThanOrEqual(110);
    expect(Math.min(...Object.values(dense.bundles))).toBeGreaterThanOrEqual(104);
    await expect(drawer.locator('.pm-th-fill,.pm-td-fill')).toHaveCount(0);
    expect(Math.max(...dense.nameLines)).toBeLessThanOrEqual(18);

    await drawer.getByRole('button',{name:/^Unavailable \/ N\/A\s+2$/}).click();
    const notApplicableRow = drawer.getByRole('row').filter({hasText:'fixture-host-02'});
    await expect(notApplicableRow.getByRole('cell').nth(1)).toHaveText('N/A');
    const unresolvedRow = drawer.getByRole('row').filter({hasText:'fixture-host-03'});
    await expect(unresolvedRow.getByRole('cell').nth(1)).toHaveText('Unknown');

    await drawer.getByRole('button',{name:/^Failing\s+1$/}).click();
    await drawer.getByRole('button',{name:'Name',exact:true}).click();
    const row=drawer.getByRole('row').filter({hasText:'fixture-host-00'});
    await expect(row.getByText('125/128',{exact:true})).toBeVisible();
    await expect(row.getByText('93/96',{exact:true})).toBeVisible();
    const failingBundle = row.getByRole('button',{name:/93\/96.*1 failed/});
    await expect(failingBundle.getByTitle('1 failed controls',{exact:true})).toBeVisible();
    await failingBundle.click();
    await expect(drawer.getByRole('button',{name:/^Controls\s+128$/})).toBeVisible();
    await expect(drawer.getByRole('button',{name:/^Failing\s+1$/})).toHaveClass('active');
    await expect(drawer.getByRole('button',{name:'Name',exact:true})).toHaveClass('active');
    await expect(drawer.getByText('Configuration:',{exact:false})).toHaveCount(0);
    await drawer.getByRole('button',{name:/columns.*Show all/}).click();
    await expect(drawer.getByRole('columnheader')).toHaveCount(98);
    expect(await drawer.getByRole('table').evaluate(table => {
      const scroll = table.parentElement;
      return scroll.scrollWidth > scroll.clientWidth;
    })).toBe(true);
    expect(await drawer.getByRole('columnheader',{name:'System',exact:true}).evaluate(node => getComputedStyle(node).position)).toBe('sticky');
    expect(await row.getByRole('cell').filter({hasText:'fixture-host-00'}).evaluate(node => getComputedStyle(node).position)).toBe('sticky');
    const stickyHealth=row.getByRole('cell').nth(1);
    expect(await stickyHealth.evaluate(node=>getComputedStyle(node).position)).toBe('sticky');
    await expect(stickyHealth.getByTitle('1 failed · 1 warning · 1 unknown',{exact:true})).toBeVisible();
    const beforeScroll=await stickyHealth.boundingBox();
    await drawer.getByRole('table').evaluate(table=>{table.parentElement.scrollLeft=table.parentElement.scrollWidth;});
    await expect.poll(()=>drawer.getByRole('table').evaluate(table=>table.parentElement.scrollLeft)).toBeGreaterThan(0);
    await expect.poll(async()=>Math.round((await stickyHealth.boundingBox()).x)).toBe(Math.round(beforeScroll.x));
    await expect(stickyHealth.getByTitle('1 failed · 1 warning · 1 unknown',{exact:true})).toBeVisible();
    await drawer.getByRole('table').evaluate(table=>{table.parentElement.scrollLeft=0;});
    await row.getByRole('button',{name:'fixture-host-00',exact:true}).click();
    const expandedDetails=drawer.locator('.pm-expand');
    await expect(expandedDetails.locator('.pm-expand-meta')).toHaveCount(0);
    for(const label of ['Configuration:','Operator classification:','Assigned bundles:']) {
      await expect(expandedDetails.getByText(label,{exact:false})).toHaveCount(0);
    }
    await drawer.getByRole('button',{name:'Fixture policy 000 · 1.0',exact:true}).click();
    await expect(drawer.getByText('[FIXTURE] services.openssh.settings.PermitRootLogin must equal no',{exact:true})).toBeVisible();
    await capture('eval-matrix-controls-detail');
    await drawer.getByRole('button',{name:/^Bundles\s+6$/}).click();
    await expect(drawer.getByRole('columnheader')).toHaveCount(8);
    await expect(drawer.getByRole('button',{name:/^Failing\s+1$/})).toHaveClass('active');

    // Same captured attempt, newly available outcome. Polling must update both
    // health denominators while retaining filter, sort, mode and row expansion.
    response={...fixture,matrix:{...fixture.matrix,rows:fixture.matrix.rows.map((r,i)=>
      i===0 ? {...r,cells:r.cells.map((status,c)=>c===32 ? 'pass' : status)} : r)}};
    expect(response.matrix.evaluation_attempt_id).toBe(fixture.matrix.evaluation_attempt_id);
    await expect(row.getByText('126/128',{exact:true})).toBeVisible({timeout:12000});
    await expect(row.getByRole('button',{name:/94\/96.*1 failed/})).toBeVisible();
    await expect(drawer.getByRole('button',{name:/^Failing\s+1$/})).toHaveClass('active');
    await expect(drawer.getByRole('button',{name:'Name',exact:true})).toHaveClass('active');
    await expect(drawer.getByRole('columnheader')).toHaveCount(8);
    await expect(drawer.getByText('[FIXTURE] services.openssh.settings.PermitRootLogin must equal no',{exact:true})).toBeVisible();

    const reopen=async()=>{
      await drawer.getByRole('button',{name:'Log',exact:true}).click();
      await drawer.getByRole('button',{name:'Policy matrix',exact:true}).click();
    };
    response={...fixture,matrix:{...fixture.matrix,columns:[],bundles:[],rows:[],
      ...inventoryFixture([],{passed:0,failed:0,evaluation_failed:0,pending:0,excluded:0})}};
    pending=new Promise(resolve=>{release=resolve;});
    await reopen();
    await expect(drawer.getByText('Loading policy matrix...',{exact:true})).toBeVisible();
    release();
    pending=null;
    await expect(drawer.getByText('No captured tracked systems for this attempt.',{exact:true})).toBeVisible();
    await expect(drawer.locator('tbody tr.pm-row')).toHaveCount(0);
    await expect(drawer.getByText(/No eligible control columns were captured/)).toHaveCount(0);
    await assertHeaders();
    await expect(drawer.getByText('Discovery pending',{exact:true})).toHaveCount(0);
    response={...fixture,matrix:null};
    await reopen();
    await expect(drawer.getByText(/Historical bundle provenance unavailable/)).toBeVisible();
    await assertHeaders();
    await expect(drawer.getByRole('button',{name:/^Bundles\s+6$/})).toHaveCount(0);
    response=fixture;
    await reopen();
    await expect(drawer.getByText('[FIXTURE] services.openssh.settings.PermitRootLogin must equal no',{exact:true})).toBeVisible();

    // A 503 unmounts evidence, not owner presentation. Recovery must restore
    // choices and the open assertion without a tab switch or manual reapply.
    fail=true;
    await expect(drawer.getByRole('alert')).toContainText('Failed to load policy matrix',{timeout:12000});
    fail=false;
    await expect(drawer.getByRole('button',{name:'Name',exact:true})).toHaveClass('active',{timeout:12000});
    await expect(drawer.getByRole('button',{name:/^Failing\s+1$/})).toHaveClass('active');
    await expect(drawer.getByText('[FIXTURE] services.openssh.settings.PermitRootLogin must equal no',{exact:true})).toBeVisible();

    // Shared controls expose each actual contributing context, never a first
    // bundle shortcut. Compliance reads the exact retained attempt and Back
    // returns to this open row, assertion, mode, sort and filter.
    await expect(drawer.getByRole('button',{name:/^View in compliance evidence · Fixture bundle/})).toHaveCount(3);
    // Browser-local receiver proof: the same shared control has two distinct
    // captured bundle contexts. Each dismissal must restore its exact opener.
    const listState = {
      search: await page.getByPlaceholder('Search queue…').inputValue(),
      latest: await page.getByRole('button',{name:'Latest per flake',exact:true}).getAttribute('aria-pressed'),
    };
    pinnedResponse=response;
    // Use a real short viewport so one expanded failing row overflows the
    // matrix's scroll container. Restore reference geometry after this block.
    await page.setViewportSize({...originalViewport,height:640});
    try {
      for (const bundleIndex of [0,1]) {
        const bundle=fixture.matrix.bundles[bundleIndex];
        for (const method of ['X','Escape','Backdrop']) {
          // Enter the drilldown explicitly: changing mode clears bundle focus.
          await drawer.getByRole('button',{name:/^Bundles\s+6$/}).click();
          await row.getByRole('button',{name:/93\/96.*1 failed/}).click();
          await expect(drawer.getByRole('button',{name:/^Controls\s+128$/})).toHaveClass('active');
          await expect(drawer.getByRole('button',{name:'Fixture bundle 1 · v1 ×',exact:true})).toBeVisible();
          await expect(drawer.getByRole('columnheader')).toHaveCount(98);
          const opener=drawer.getByRole('button',{name:`View in compliance evidence · ${bundle.name} · ${bundle.version}`,exact:true});
          const openerId=await opener.getAttribute('id');
          expect(openerId).toBeTruthy();
          await expect.poll(()=>drawer.locator('.pm-scroll').evaluate(el=>el.scrollHeight>el.clientHeight)).toBe(true);
          await drawer.locator('.pm-scroll').evaluate(el=>{el.scrollTop=80;});
          // Native focus can scroll the opener into view. Save that final offset.
          await opener.focus();
          const scroll=await drawer.locator('.pm-scroll').evaluate(el=>el.scrollTop);
          expect(scroll).toBeGreaterThan(0);
          await opener.press('Enter');
          const evidence=page.getByRole('dialog',{name:'Evaluation check evidence',exact:true});
          const exact=page.getByRole('region',{name:'Exact evaluation evidence'});
          await expect(exact).toContainText(fixture.matrix.evaluation_attempt_id);
          await expect(exact).toContainText(bundle.bundle_id);
          await expect(exact).toContainText(bundle.bundle_version_id);
          await expect(exact).toContainText(`${bundle.name} · ${bundle.version}`);
          await expect(exact).toContainText(bundle.framework);
          await expect(exact).toContainText(bundle.framework_version);
          const params=new URL(page.url()).searchParams;
          expect(params.get('bundle')).toBe(bundle.bundle_id);
          expect(params.get('version')).toBe(bundle.bundle_version_id);
          expect(params.get('policy')).toBe(fixture.matrix.columns[0].policy_version_id);
          await assertDialogKeyboard(page,evidence,'compliance-evaluation-dialog');
          if (method==='X') {
            // The receiver steps through exactly the controls this captured system
            // received through this captured bundle version, in snapshot order.
            const owned=fixture.matrix.columns.filter((_,index)=>
              fixture.matrix.rows[0].policies[index].memberships.some(m=>m.bundle_version_id===bundle.bundle_version_id));
            expect(owned.length).toBe(bundleIndex===0 ? 96 : 32);
            const controlNav=evidence.getByRole('navigation',{name:'Controls in this bundle'});
            const policyParam=()=>new URL(page.url()).searchParams.get('policy');
            await expect(evidence.getByText(`Stepping through ${owned.length} controls`)).toBeVisible();
            const identity=evidence.getByTestId('ce-evidence-identity');
            await expect(identity).toContainText('fixture-host-00');
            await expect(identity).toContainText(bundle.name);
            const navButtons=controlNav.getByRole('button');
            await expect(navButtons).toHaveCount(owned.length);
            await expect(controlNav.getByText('Grouping is unavailable',{exact:false})).toBeVisible();
            await expect(navButtons.filter({hasText:owned[0].name})).toHaveAttribute('aria-current','true');
            if (bundleIndex===0 && captureEvidence) await captureEvidence('eval-evidence-navigation');
            // j and k step within the captured list and replace the history entry.
            const historyLength=await page.evaluate(()=>history.length);
            await page.keyboard.press('j');
            await expect.poll(policyParam).toBe(owned[1].policy_version_id);
            await expect(exact).toContainText(owned[1].name);
            await expect(navButtons.filter({hasText:owned[1].name})).toHaveAttribute('aria-current','true');
            await page.keyboard.press('k');
            await expect.poll(policyParam).toBe(owned[0].policy_version_id);
            await page.keyboard.press('k');
            await expect.poll(policyParam).toBe(owned[0].policy_version_id);
            expect(await page.evaluate(()=>history.length)).toBe(historyLength);
            // A click selects directly. The sibling bundle's own controls never appear.
            await navButtons.filter({hasText:owned[2].name}).click();
            await expect.poll(policyParam).toBe(owned[2].policy_version_id);
            await expect(exact).toContainText(owned[2].name);
            await expect(navButtons.filter({hasText:owned[2].name})).toHaveAttribute('aria-current','true');
            await expect(navButtons.filter({hasText:'Fixture policy 127'})).toHaveCount(0);
            // Arrow keys on a focused list item use the same captured visible
            // order as j/k. Navigation does not wrap at the list edge.
            const selectedNavItem=navButtons.filter({hasText:owned[2].name});
            await selectedNavItem.focus();
            await page.keyboard.press('ArrowDown');
            await expect.poll(policyParam).toBe(owned[3].policy_version_id);
            await expect(navButtons.filter({hasText:owned[3].name})).toHaveAttribute('aria-current','true');
            await page.keyboard.press('ArrowUp');
            await expect.poll(policyParam).toBe(owned[2].policy_version_id);
            await expect(navButtons.filter({hasText:owned[2].name})).toHaveAttribute('aria-current','true');
            // The filter narrows the list. Keys typed or pressed there never step.
            const filter=evidence.getByRole('searchbox',{name:'Filter controls'});
            await filter.fill('Fixture policy 00');
            await expect(navButtons).toHaveCount(10);
            await filter.press('j');
            await filter.press('k');
            await expect(filter).toHaveValue('Fixture policy 00jk');
            await expect.poll(policyParam).toBe(owned[2].policy_version_id);
            await filter.fill('Fixture policy 00');
            await filter.press('ArrowDown');
            await filter.press('ArrowDown');
            await expect.poll(policyParam).toBe(owned[2].policy_version_id);
            await expect(evidence).toBeVisible();
            await filter.fill('absent control');
            await expect(controlNav.getByText('No controls match.',{exact:true})).toBeVisible();
            await filter.fill('');
            await expect(navButtons).toHaveCount(owned.length);
            // Expand and Restore resize the receiver without leaving the evidence.
            const widthNow=async()=>(await evidence.boundingBox()).width;
            await evidence.getByRole('button',{name:'Expand',exact:true}).click();
            await expect.poll(widthNow).toBe(Math.min(1360,page.viewportSize().width));
            if (bundleIndex===0 && captureEvidence) await captureEvidence('eval-evidence-navigation-maximized');
            await evidence.getByRole('button',{name:'Restore',exact:true}).click();
            await expect.poll(widthNow).toBe(Math.min(960,page.viewportSize().width*0.96));
            if (bundleIndex===0) {
              await navButtons.filter({hasText:owned[0].name}).click();
              await expect.poll(policyParam).toBe(owned[0].policy_version_id);
              const viewport=page.viewportSize();
              await page.setViewportSize({width:720,height:viewport.height});
              const narrowBox=await evidence.boundingBox();
              expect(narrowBox.width).toBeLessThanOrEqual(720);
              await expect(controlNav).toBeVisible();
              await expect(exact).toBeVisible();
              await captureEvidence('eval-evidence-navigation-narrow');
              await page.setViewportSize(viewport);
            }
          }
          response={...fixture,matrix:{...fixture.matrix,evaluation_attempt_id:id(9002),
            attempt_number:fixture.matrix.attempt_number+1,columns:[],bundles:[],rows:[],
            ...inventoryFixture([],{passed:0,failed:0,evaluation_failed:0,pending:0,excluded:0})}};
          await dismissDialog(page,evidence,method,'Close evaluation evidence');
          await expect(evidence).toBeHidden();
          expect(new URL(page.url()).pathname).toBe(`/evaluations/${commitId}/attempts/${fixture.matrix.evaluation_attempt_id}`);
          await expect(drawer).toHaveAttribute('data-commit-sha',fixture.matrix.commit_sha);
          await expect(drawer).toHaveAttribute('data-attempt-number',String(fixture.matrix.attempt_number));
          await expect(drawer.getByRole('button',{name:/^Controls\s+128$/})).toHaveClass('active');
          await expect(drawer.getByRole('button',{name:/^Failing\s+1$/})).toHaveClass('active');
          await expect(drawer.getByRole('button',{name:'Name',exact:true})).toHaveClass('active');
          await expect(drawer.getByRole('button',{name:'fixture-host-00',exact:true})).toHaveAttribute('aria-expanded','true');
          await expect(drawer.getByText('[FIXTURE] services.openssh.settings.PermitRootLogin must equal no',{exact:true})).toBeVisible();
          await expect(page.getByPlaceholder('Search queue…')).toHaveValue(listState.search);
          await expect(page.getByRole('button',{name:'Latest per flake',exact:true})).toHaveAttribute('aria-pressed',listState.latest);
          await expect.poll(()=>drawer.locator('.pm-scroll').evaluate(el=>el.scrollTop)).toBe(scroll);
          await expect(opener).toBeFocused();
          await expect(drawer.getByRole('button',{name:'Fixture bundle 1 · v1 ×',exact:true})).toBeVisible();
          await expect(drawer.getByRole('button',{name:'Failing only',exact:true})).not.toHaveClass(/active-filter/);
          await expect(drawer.getByRole('columnheader')).toHaveCount(98);
          await drawer.getByRole('button',{name:/^Bundles\s+6$/}).click();
        }
      }
    } finally {
      await page.setViewportSize(originalViewport);
    }
    response=fixture;
    pinnedResponse=null;
    await drawer.getByRole('button',{name:'View in compliance evidence · Fixture bundle 2 · v1',exact:true}).click();
    await expect(page.getByRole('heading',{name:'Evaluation check evidence',exact:true})).toBeVisible();
    const evidenceUrl=new URL(page.url());
    expect(evidenceUrl.pathname).toBe(`/compliance/evaluations/${commitId}/attempts/${fixture.matrix.evaluation_attempt_id}`);
    expect(Object.fromEntries(evidenceUrl.searchParams)).toEqual({
      bundle:fixture.matrix.bundles[1].bundle_id,version:fixture.matrix.bundles[1].bundle_version_id,
      system:fixture.matrix.rows[0].system_id,policy:fixture.matrix.columns[0].policy_version_id,
    });
    await expect(page.getByRole('region',{name:'Exact evaluation evidence'})).toContainText(fixture.matrix.commit_sha);
    await expect(page.getByText('Mock evaluation — simulated outcomes, not compliance evidence.',{exact:true})).toBeVisible();
    // A retry becomes the latest matrix while the receiver is open. Back must
    // keep reading the retained UUID, not apply old choices to the new attempt.
    pinnedResponse=response;
    response={...fixture,matrix:{...fixture.matrix,evaluation_attempt_id:id(9002),attempt_number:fixture.matrix.attempt_number+1,rows:[],
      ...inventoryFixture([],{passed:0,failed:0,evaluation_failed:0,pending:0,excluded:0})}};
    await page.getByRole('button',{name:'Back to Evaluation',exact:true}).click();
    await expect(drawer.getByRole('button',{name:'Name',exact:true})).toHaveClass('active');
    await expect(drawer.getByRole('button',{name:/^Failing\s+1$/})).toHaveClass('active');
    await expect(drawer.getByText('[FIXTURE] services.openssh.settings.PermitRootLogin must equal no',{exact:true})).toBeVisible();
    expect(pinnedRequests.length).toBeGreaterThanOrEqual(2);
    expect(pinnedRequests.every(url=>new URL(url).pathname.endsWith(`/commits/${commitId}/eval/attempts/${fixture.matrix.evaluation_attempt_id}/policy-matrix`))).toBe(true);
    await expect(drawer).toHaveAttribute('data-attempt-number',String(fixture.matrix.attempt_number));
    response=fixture;
    pinnedResponse=null;

    await drawer.getByRole('button',{name:'Open system',exact:true}).click();
    await expect(page).toHaveURL(new RegExp(`/systems/${fixture.matrix.rows[0].system_id}\\?.*revision=${fixture.matrix.commit_sha}`));
    // This browser-local UUID is not a persisted system. Even the unavailable
    // receiver must retain the exact-context return action.
    await page.getByRole('link',{name:'Back to Evaluation',exact:true}).click();
    await expect(drawer.getByRole('button',{name:'Name',exact:true})).toHaveClass('active');
    await expect(drawer.getByText('[FIXTURE] services.openssh.settings.PermitRootLogin must equal no',{exact:true})).toBeVisible();
    await expect(drawer.getByRole('button',{name:'Log unavailable for retained attempt',exact:true})).toBeDisabled();

    // The receiver's own Open system keeps the validated exact-context return.
    await drawer.getByRole('button',{name:'View in compliance evidence · Fixture bundle 2 · v1',exact:true}).click();
    const headerReceiver=page.getByRole('dialog',{name:'Evaluation check evidence',exact:true});
    await expect(page.getByRole('region',{name:'Exact evaluation evidence'})).toContainText(fixture.matrix.commit_sha);
    await headerReceiver.getByRole('button',{name:'Open system',exact:true}).click();
    await expect(page).toHaveURL(new RegExp(`/systems/${fixture.matrix.rows[0].system_id}\\?.*revision=${fixture.matrix.commit_sha}`));
    await page.getByRole('link',{name:'Back to Evaluation',exact:true}).click();
    await expect(drawer.getByRole('button',{name:'Name',exact:true})).toHaveClass('active');
    await expect(drawer.getByText('[FIXTURE] services.openssh.settings.PermitRootLogin must equal no',{exact:true})).toBeVisible();

    // Missing captured contributions fail closed without a deployed fallback.
    pinnedResponse={...fixture,matrix:{...fixture.matrix,rows:fixture.matrix.rows.map((r,index)=>index===0 ? {...r,policies:r.policies.map((p,i)=>i===0 ? {...p,memberships:[]} : p)} : r)}};
    await drawer.getByRole('button',{name:'View in compliance evidence · Fixture bundle 2 · v1',exact:true}).click();
    await expect(page.getByRole('dialog',{name:'Evaluation check evidence',exact:true}).getByRole('alert')).toContainText('exact captured context or membership does not match');
    await expect(page.getByRole('region',{name:'Exact evaluation evidence'})).toHaveCount(0);
    await expect(page.getByRole('button',{name:'Back to Evaluation',exact:true})).toBeDisabled();
    // An invalid contribution cannot restore a validated rich return. X exits
    // through the explicit Compliance fallback; reset the fixture only afterward.
    await dismissDialog(page,page.getByRole('dialog',{name:'Evaluation check evidence',exact:true}),'X','Close evaluation evidence');
    await expect.poll(()=>new URL(page.url()).pathname).toBe('/compliance');
    await expect(page.getByRole('dialog',{name:'Evaluation check evidence',exact:true})).toBeHidden();
    await expect(page.getByRole('region',{name:'Exact evaluation evidence'})).toHaveCount(0);
    pinnedResponse=null;
    response=fixture;
    await page.goto(new URL('/evaluations',page.url()).href);
    await page.getByRole('button',{name:/^Active Queue\b/}).click();
    await page.getByRole('row').filter({hasText:fixture.matrix.commit_sha.slice(0,12)}).click();
    await drawer.getByRole('button',{name:'Policy matrix',exact:true}).click();
    await drawer.getByRole('button',{name:'Name',exact:true}).click();

    // Full API discovery still drives the header; only captured tracked systems
    // belong in matrix rows and configuration filters.
    const unmanaged={system_id:null,system_name:'fixture-unmanaged-config',
      configuration_name:'fixture-unmanaged-config',resolution:'unavailable',
      evaluation_digest:null,assigned_bundles:[],policies:[],
      cells:fixture.matrix.columns.map(()=> 'unknown'),details:fixture.matrix.columns.map(()=>null)};
    const reconciledRows=[...fixture.matrix.rows,unmanaged];
    response={...fixture,matrix:{...fixture.matrix,rows:reconciledRows,
      ...inventoryFixture(reconciledRows,{passed:58,failed:1,evaluation_failed:0,pending:1,unavailable:1,excluded:0})}};
    await reopen();
    await drawer.getByRole('button',{name:/^All\s+60$/}).click();
    await assertNames(expectedNames);
    await assertHeaders();
    await expect(drawer.getByRole('button',{name:'fixture-unmanaged-config',exact:true})).toHaveCount(0);
    await expect(drawer.getByRole('table').getByText(/·\s*Config\b/)).toHaveCount(0);
    await capture('eval-matrix-tracked-only');

    // Available discovery without captured system mappings is not filter-zero.
    response={...fixture,matrix:{...fixture.matrix,rows:[unmanaged],
      ...inventoryFixture([unmanaged],{passed:0,failed:0,evaluation_failed:0,pending:0,unavailable:1,excluded:0})}};
    await reopen();
    await expect(drawer.getByText('No captured tracked systems for this attempt.',{exact:true})).toBeVisible();
    await expect(drawer.getByText('No systems match this filter.',{exact:true})).toHaveCount(0);
    await expect(drawer.getByRole('button',{name:'fixture-unmanaged-config',exact:true})).toHaveCount(0);
    await assertHeaders();

    // Header coverage retains the full union. Matrix filters count three mapped
    // configurations across four host rows, retaining shared assignments.
    const unionRows=fixture.matrix.rows.slice(4,7).map((r,i)=>({...r,
      cells:r.cells.map((_,c)=>i===2 && c===0 ? 'fail' : 'pass'),details:r.details.map(()=>null)}));
    const shared={...unionRows[2],system_id:id(6999),system_name:'fixture-shared-host',cells:unionRows[2].cells.map(()=> 'pass')};
    const discoveredOnly={...unmanaged,system_name:'fixture-discovery-only',configuration_name:'fixture-discovery-only'};
    response={...fixture,matrix:{...fixture.matrix,rows:[...unionRows,shared,discoveredOnly],
      inventory:{execution_mode:'mock',configurations:['fixture-config-5','fixture-config-6','fixture-discovery-only'],
        selected_configurations:['fixture-config-6','fixture-discovery-only']},
      configuration_summary:{total:4,passed:0,failed:1,evaluation_failed:0,pending:1,unavailable:1,excluded:1}}};
    await reopen();
    await drawer.getByRole('button',{name:/^All\s+3$/}).click();
    await assertNames(['fixture-host-04','fixture-host-05','fixture-host-06','fixture-shared-host']);
    await expect(drawer.getByRole('button',{name:'fixture-discovery-only',exact:true})).toHaveCount(0);
    await expect(drawer.getByRole('table').getByText(/·\s*Config\b/)).toHaveCount(0);
    await assertHeaders();
    for(const name of ['fixture-host-04','fixture-host-05']) {
      const unavailable=drawer.getByRole('row').filter({has:page.getByRole('button',{name,exact:true})});
      await expect(unavailable.getByRole('cell').nth(1)).toHaveText('Unknown');
    }
    await expect(drawer.getByRole('button',{name:'fixture-host-04',exact:true})).toHaveAttribute('title','Not discovered');
    await expect(drawer.getByRole('button',{name:'fixture-host-05',exact:true})).toHaveAttribute('title','Not selected');
    for(const name of ['fixture-host-06','fixture-shared-host']) {
      await expect(drawer.getByRole('button',{name,exact:true})).toHaveAttribute('title','Shared configuration');
      await expect(drawer.getByRole('button',{name,exact:true})).toContainText('↔');
    }
    await drawer.getByRole('button',{name:/^Failing\s+1$/}).click();
    await assertNames(['fixture-host-06','fixture-shared-host']);
    await expect(drawer.getByRole('button',{name:/^Unavailable \/ N\/A\s+2$/})).toBeVisible();

    // The absent planned config is already unavailable. Once this exact attempt
    // is terminal, the discovery-only config cannot complete either.
    response={...response,matrix:{...response.matrix,attempt_status:'complete',
      configuration_summary:{total:4,passed:0,failed:1,evaluation_failed:0,pending:0,unavailable:2,excluded:1}}};
    await reopen();
    await assertHeaders();
    const policyStat=drawer.locator('.ed-stat').filter({has:page.getByText('Policy',{exact:true})});
    await expect(policyStat.getByText('2 unavailable',{exact:true})).toBeVisible();
    await expect(policyStat.getByText(/pending|No results/)).toHaveCount(0);

    // A wider Controls fixture bounds current DOM without dropping systems.
    const extra=fixture.matrix.columns.map((c,i)=>({...c,policy_version_id:id(10001+i),policy_id:id(11001+i),name:`Extra fixture ${i}`}));
    response={...fixture,matrix:{...fixture.matrix,columns:[...fixture.matrix.columns,...extra],rows:fixture.matrix.rows.map(r=>({...r,cells:[...r.cells,...extra.map(()=> 'pass')],policies:[...r.policies,...extra.map(c=>({policy_version_id:c.policy_version_id,report_only:false,executable:true,memberships:[]}))]}))}};
    await reopen();
    await drawer.getByRole('button',{name:/^All\s+60$/}).click();
    await drawer.getByRole('button',{name:/^Controls\s+256$/}).click();
    const trim=drawer.getByRole('button',{name:'Failing only',exact:true});
    if(await trim.getAttribute('class').then(c=>c.includes('active-filter'))) await trim.click();
    const pager=drawer.getByRole('navigation',{name:'Matrix system pages'});
    await expect(pager).toBeVisible();
    await assertHeaders();
    // Measure only data rows; collapse any retained row detail first.
    const hostToggle=drawer.getByRole('button',{name:'fixture-host-00',exact:true});
    if(await hostToggle.getAttribute('aria-expanded')==='true') await hostToggle.click();
    const cells=await drawer.getByRole('table').getByRole('cell').count();
    expect(cells).toBeLessThanOrEqual(8192);
    await pager.getByRole('button',{name:'Next systems',exact:true}).click();
    await expect(drawer.getByRole('button',{name:'fixture-host-59',exact:true})).toBeVisible();

    // Only an explicit greenfield classification excludes a system from trim.
    // Unknown classification stays in the baseline; health still counts all rows.
    const classified=fixture.matrix.rows.slice(0,3).map((r,index)=>({...r,
      resolution:'resolved',policies:fixture.matrix.rows[0].policies,
      greenfield:index===0 ? true : index===1 ? false : null,
      cells:r.cells.map((_,column)=>index===0 && column===0 ? 'fail' : index===1 && column===16 ? 'warn' : index===2 && column===32 ? 'unknown' : 'pass'),
      details:r.details.map(()=>null),
    }));
    response={...fixture,matrix:{...fixture.matrix,rows:classified,
      ...inventoryFixture(classified,{passed:1,failed:1,evaluation_failed:0,pending:1,excluded:0})}};
    await reopen();
    await drawer.getByRole('button',{name:/^All\s+3$/}).click();
    await drawer.getByRole('button',{name:/^Controls\s+128$/}).click();
    if(!await trim.getAttribute('class').then(c=>c.includes('active-filter'))) await trim.click();
    await expect(drawer.getByRole('columnheader')).toHaveCount(4);
    await expect(drawer.getByRole('button',{name:/^Failing\s+1$/})).toBeVisible();
    await expect(drawer.getByRole('button',{name:/^Unavailable \/ N\/A\s+1$/})).toBeVisible();
    await assertHeaders();
    // When every system is explicitly greenfield, keep the full baseline rather
    // than erase every interesting column.
    response={...response,matrix:{...response.matrix,rows:classified.map(r=>({...r,greenfield:true}))}};
    await reopen();
    await expect(drawer.getByRole('columnheader')).toHaveCount(5);

    // Sixteen controls make the single-bundle selector legitimate (>14).
    // Its inventory intentionally changes to twelve configurations.
    const only=fixture.matrix.bundles[3];
    const onlyContext=fixture.matrix.rows[0].assigned_bundles[3];
    response={...fixture,matrix:{...fixture.matrix,columns:fixture.matrix.columns.slice(0,16),bundles:[only],
      rows:fixture.matrix.rows.slice(4,16).map((r,index)=>({...r,resolution:'resolved',greenfield:null,
        cells:Array.from({length:16},(_,c)=>index===0 && c===0 ? 'fail' : index===0 && c===1 ? 'warn' : 'pass'),
        details:Array.from({length:16},(_,c)=>index===0 && c===0 ? '[FIXTURE] captured failure detail' : null),
        assigned_bundles:[onlyContext],
        policies:r.policies.slice(0,16).map((p,c)=>({...p,report_only:c===1,memberships:[onlyContext]}))}))}};
    Object.assign(response.matrix,inventoryFixture(response.matrix.rows,{passed:11,failed:1,evaluation_failed:0,pending:0,excluded:0}));
    await reopen();
    await drawer.getByRole('button',{name:/^All\s+12$/}).click();
    await drawer.getByRole('button',{name:/^Bundles\s+1$/}).click();
    await expect(drawer.getByRole('columnheader')).toHaveCount(3);
    await assertNames(expectedNames.slice(4,16));
    await assertNoInventedPanels();
    const sparse=await geometry();
    expect(sparse.system).toBeGreaterThanOrEqual(180);
    expect(sparse.health).toBeGreaterThanOrEqual(110);
    expect(Math.abs(sparse.row-dense.row)).toBeLessThanOrEqual(1);
    expect(Object.values(sparse.bundles)[0]).toBeGreaterThanOrEqual(104);
    await expect(drawer.locator('.pm-th-fill,.pm-td-fill')).toHaveCount(0);
    await capture('eval-matrix-single-bundle');

    // Failure-first detail uses saved diagnostics without inventing assertions.
    await drawer.getByRole('button',{name:'fixture-host-04',exact:true}).click();
    const cards=drawer.locator('.pm-failcard-title');
    await expect(cards).toHaveText(['Fixture policy 000 · 1.0','Fixture policy 001 · 1.0']);
    await expect(drawer.getByRole('button',{name:'Show 14 passed and N/A controls',exact:true})).toBeVisible();
    await drawer.getByRole('button',{name:'Fixture policy 000 · 1.0',exact:true}).click();
    await expect(drawer.getByText('[FIXTURE] captured failure detail',{exact:true})).toBeVisible();
    await assertNoInventedPanels();
    await expect(drawer.getByRole('button',{name:'View in compliance evidence · Fixture bundle 4 · v1',exact:true})).toBeVisible();
    await capture('eval-matrix-failure-first');

    // New browser-local attempt identities give the one/two-control fixtures
    // genuine default Controls state, without inherited dense column choices.
    const singleBundle=response.matrix;
    for(const count of [1,2]) {
      response={...fixture,matrix:{...singleBundle,evaluation_attempt_id:id(9100+count),
        columns:singleBundle.columns.slice(0,count),rows:singleBundle.rows.map(r=>({...r,
          cells:r.cells.slice(0,count),details:r.details.slice(0,count),policies:r.policies.slice(0,count)}))}};
      await reopen();
      await expect(drawer.getByRole('columnheader')).toHaveCount(count+2);
      await expect(drawer.getByRole('columnheader',{name:/Fixture policy 000/})).toBeVisible();
      await expect(drawer.getByRole('button',{name:/^Bundles\s+1$/})).toBeVisible();
      await expect(drawer.getByRole('button',{name:new RegExp(`^Controls\\s+${count}$`)})).toBeVisible();
      await drawer.getByRole('button',{name:/^Bundles\s+1$/}).click();
      await expect(drawer.getByRole('columnheader')).toHaveCount(3);
      await drawer.getByRole('button',{name:new RegExp(`^Controls\\s+${count}$`)}).click();
      await expect(drawer.getByRole('columnheader')).toHaveCount(count+2);
      await expect(drawer.getByRole('button',{name:'Failing only',exact:true})).toBeVisible();
      await expect(drawer.locator('.pm-controls-line')).toHaveCount(2);
      await drawer.getByRole('button',{name:'Name',exact:true}).click();
      await assertNames(expectedNames.slice(4,16));
      const sparseControls=await drawer.getByRole('table').evaluate(table=>{
        const heads=table.querySelectorAll('thead th');
        return {system:heads[0].getBoundingClientRect().width,health:heads[1].getBoundingClientRect().width,
          width:table.getBoundingClientRect().width,available:table.parentElement.clientWidth,
          filler:table.querySelectorAll('.pm-th-fill,.pm-td-fill').length};
      });
      expect(sparseControls.system).toBeGreaterThan(180);
      expect(sparseControls.health).toBeGreaterThan(110);
      expect(Math.abs(sparseControls.width-sparseControls.available)).toBeLessThanOrEqual(1);
      expect(sparseControls.filler).toBe(0);
      await capture(`eval-matrix-sparse-${count}-controls`);
    }

    // A matching-attempt global gate failure changes configuration health, but
    // adds no policy identity or Top issues entry when all controls pass.
    const gateRows=fixture.matrix.rows.map(r=>({...r,resolution:'resolved',
      policies:fixture.matrix.rows[0].policies,cells:r.cells.map(()=> 'pass'),details:r.details.map(()=>null)}));
    response={...fixture,matrix:{...fixture.matrix,rows:gateRows,
      ...inventoryFixture(gateRows,{passed:59,failed:1,evaluation_failed:0,pending:0,excluded:0}),
      configuration_failures:[{configuration_name:'fixture-config-0',detail:'[FIXTURE] Exact-attempt Crystal Forge agent requirement failed'}]}};
    await reopen();
    await drawer.getByRole('button',{name:/^All\s+60$/}).click();
    await drawer.getByRole('button',{name:/^Bundles\s+6$/}).click();
    await assertNames(expectedNames);
    await assertHeaders();
    await expect(drawer.getByText('Top issues',{exact:true})).toHaveCount(0);
    await expect(drawer.getByRole('button',{name:/^Failing\s+1$/})).toBeVisible();
    await expect(drawer.getByRole('button',{name:/^Clean\s+59$/})).toBeVisible();
    await expect(drawer.getByRole('button',{name:/^Controls\s+128$/})).toBeVisible();
    await drawer.getByRole('button',{name:'fixture-host-00',exact:true}).click();
    await expect(drawer.getByRole('row').filter({has:page.getByRole('button',{name:'fixture-host-00',exact:true})}).getByText('128/128',{exact:true})).toBeVisible();
    await expect(drawer.getByText('Configuration requirement · Crystal Forge agent',{exact:true})).toBeVisible();
    await expect(drawer.getByText('[FIXTURE] Exact-attempt Crystal Forge agent requirement failed',{exact:true})).toBeVisible();
    await expect(drawer.getByText('✓ All applicable controls pass for this system.',{exact:true})).toBeVisible();
    await assertNoInventedPanels();
    await expect(drawer.getByRole('button',{name:'fixture-host-00',exact:true})).toHaveAttribute('title','Configuration requirement failed');
    await expect(drawer.getByRole('button',{name:'fixture-host-00',exact:true})).toContainText('!');

    // A false gate remains failed without a diagnostic. Omitted kind exercises
    // the cf_agent wire default; requirements is a distinct real global gate.
    response={...response,matrix:{...response.matrix,
      configuration_summary:{total:60,passed:58,failed:2,evaluation_failed:0,pending:0,unavailable:0,excluded:0},
      configuration_failures:[{configuration_name:'fixture-config-0',detail:''},
        {kind:'requirements',configuration_name:'fixture-config-1',detail:''}]}};
    await reopen();
    await expect(drawer.getByRole('button',{name:/^Failing\s+2$/})).toBeVisible();
    await assertHeaders();
    await expect(drawer.getByRole('button',{name:'fixture-host-00',exact:true})).toHaveAttribute('title','Configuration requirement failed');
    await expect(drawer.getByText('Configuration requirement · Crystal Forge agent',{exact:true})).toBeVisible();
    await expect(drawer.getByText('[FIXTURE] Exact-attempt Crystal Forge agent requirement failed',{exact:true})).toHaveCount(0);
    await drawer.getByRole('button',{name:'fixture-host-01',exact:true}).click();
    await expect(drawer.getByText('Configuration policy requirements',{exact:true})).toBeVisible();
    await expect(drawer.getByText('Top issues',{exact:true})).toHaveCount(0);
    await expect(drawer.getByRole('button',{name:/^Controls\s+128$/})).toBeVisible();

    // Known passing row controls do not prove an authorized configuration gate.
    // Withhold one gate without changing applicability or the row-health filter.
    response={...response,matrix:{...response.matrix,
      configuration_summary:{total:60,passed:57,failed:2,evaluation_failed:0,pending:0,unavailable:1,excluded:0}}};
    await reopen();
    await assertHeaders();
    await expect(policyStat.getByText('1 unavailable',{exact:true})).toBeVisible();
    await expect(drawer.getByRole('button',{name:/^Clean\s+58$/})).toBeVisible();

    // A new dense attempt resets operator choices. This differs from both a
    // same-UUID empty capture and a same-UUID outcome update above.
    await drawer.getByRole('button',{name:'Name',exact:true}).click();
    await drawer.getByRole('button',{name:/^Failing\s+2$/}).click();
    await drawer.getByRole('button',{name:/^Controls\s+128$/}).click();
    const resetTrim=drawer.getByRole('button',{name:'Failing only',exact:true});
    if ((await resetTrim.getAttribute('class')).includes('active-filter')) await resetTrim.click();
    response={...fixture,matrix:{...fixture.matrix,evaluation_attempt_id:id(9200)}};
    await reopen();
    await expect(drawer.getByRole('button',{name:/^Bundles\s+6$/})).toHaveClass('active');
    await expect(drawer.getByRole('button',{name:/^All\s+60$/})).toHaveClass('active');
    await expect(drawer.getByRole('button',{name:'Worst first',exact:true})).toHaveClass('active');
    await drawer.getByRole('button',{name:/^Controls\s+128$/}).click();
    await expect(resetTrim).toHaveClass(/active-filter/);
    await expect(drawer.getByRole('button',{name:'fixture-host-00',exact:true})).toHaveAttribute('aria-expanded','false');

    // Reload and a fresh JS context cannot depend on owner-held presentation.
    // They must resolve the shareable route from this exact-attempt API fixture.
    pinnedResponse=fixture;
    const evidenceRequests=pinnedRequests.length;
    await page.goto(evidenceUrl.href);
    await expect(page.getByRole('region',{name:'Exact evaluation evidence'})).toContainText(fixture.matrix.evaluation_attempt_id);
    await page.reload();
    await expect(page.getByRole('region',{name:'Exact evaluation evidence'})).toContainText(fixture.matrix.commit_sha);
    expect(pinnedRequests.length).toBeGreaterThan(evidenceRequests);
    const freshContext=await page.context().browser().newContext({storageState:await page.context().storageState()});
    try {
      const freshPage=await freshContext.newPage();
      const freshRequests=[];
      let freshResponse=fixture;
      await freshPage.route(pinnedEndpoint,route=>{
        freshRequests.push(route.request().url());
        return route.fulfill({json:freshResponse});
      });
      await freshPage.goto(evidenceUrl.href);
      await expect(freshPage.getByRole('region',{name:'Exact evaluation evidence'})).toContainText(fixture.matrix.evaluation_attempt_id);
      expect(freshRequests.length).toBe(1);
      expect(new URL(freshRequests[0]).pathname).toBe(new URL(pinnedRequests[0]).pathname);
      freshResponse={...fixture,matrix:{...fixture.matrix,evaluation_attempt_id:id(9002)}};
      await freshPage.reload();
      await expect(freshPage.getByRole('dialog',{name:'Evaluation check evidence',exact:true}).getByRole('alert')).toContainText('exact captured context or membership does not match');
      await expect(freshPage.getByRole('region',{name:'Exact evaluation evidence'})).toHaveCount(0);
      freshResponse=fixture;
      await freshPage.reload();
      await expect(freshPage.getByRole('region',{name:'Exact evaluation evidence'})).toContainText(fixture.matrix.evaluation_attempt_id);
      await freshPage.getByRole('button',{name:'Back to Evaluation',exact:true}).click();
      const retained=freshPage.getByRole('dialog',{name:'Evaluation detail',exact:true});
      await expect(retained).toBeVisible();
      await assertDialogKeyboard(freshPage,retained,'retained-evaluation-dialog');
      await expect(retained).toHaveAttribute('data-attempt-number',String(fixture.matrix.attempt_number));
      await expect(retained).toHaveAttribute('data-attempt-id',fixture.matrix.evaluation_attempt_id);
      const retainedStat=label=>retained.locator('.ed-stat').filter({has:freshPage.getByText(label,{exact:true})}).locator('.ed-stat-val');
      await expect(retainedStat('Systems')).toHaveText('60');
      await expect(retainedStat('Policy')).toHaveText(/^58\s*\/\s*1\s*1 unavailable$/);
      await expect(retained.getByRole('tab',{name:'Log unavailable for retained attempt',exact:true})).toBeDisabled();
      await expect(retained.getByRole('tab',{name:'Dependency graph unavailable',exact:true})).toBeDisabled();
      await expect(retained.getByRole('button',{name:/^All\s+60$/})).toHaveClass('active');
      expect(freshRequests.every(url=>new URL(url).pathname===new URL(pinnedRequests[0]).pathname)).toBe(true);
      // View bundle opens the exact captured bundle version in Compliance. It
      // names the catalog entry only; it never claims the capture still matches.
      await freshPage.goto(evidenceUrl.href);
      await expect(freshPage.getByRole('region',{name:'Exact evaluation evidence'})).toContainText(fixture.matrix.evaluation_attempt_id);
      await freshPage.getByRole('dialog',{name:'Evaluation check evidence',exact:true}).getByRole('button',{name:'View bundle',exact:true}).click();
      await expect.poll(()=>new URL(freshPage.url()).pathname).toBe('/compliance');
      expect(Object.fromEntries(new URL(freshPage.url()).searchParams)).toMatchObject({
        bundle:fixture.matrix.bundles[1].bundle_id,version:fixture.matrix.bundles[1].bundle_version_id,
      });
      await expect(freshPage.getByRole('dialog',{name:'Evaluation check evidence',exact:true})).toBeHidden();
      for (const method of ['Escape','X','Backdrop']) {
        await freshPage.goto(evidenceUrl.href);
        const directEvidence=freshPage.getByRole('dialog',{name:'Evaluation check evidence',exact:true});
        await expect(freshPage.getByRole('region',{name:'Exact evaluation evidence'})).toContainText(fixture.matrix.evaluation_attempt_id);
        await assertDialogKeyboard(freshPage,directEvidence,'compliance-evaluation-dialog');
        await dismissDialog(freshPage,directEvidence,method,'Close evaluation evidence');
        await expect(directEvidence).toBeHidden();
        await expect(retained).toHaveAttribute('data-attempt-id',fixture.matrix.evaluation_attempt_id);
        await assertDialogKeyboard(freshPage,retained,'retained-evaluation-dialog');
        await dismissDialog(freshPage,retained,method);
        await expect(retained).toBeHidden();
        // Direct URLs have no connected opener. dialog_focus.rs specifies the
        // main heading as the focus fallback in this case.
        await expect(freshPage.getByRole('heading',{name:'Evaluations',exact:true})).toBeFocused();
      }
    } finally {
      await freshContext.close();
    }
    // Completed owns additional page filters. Exercise their non-default values
    // through the same evidence dismissals, independently of the queue case.
    response=fixture;
    pinnedResponse=fixture;
    onHistoryPhase(true);
    await page.goto(new URL('/evaluations',evidenceUrl).href);
    await page.getByRole('button',{name:/^Completed\b/}).click();
    const completedSearch=page.getByPlaceholder('Search completed…');
    await completedSearch.fill('TASK-488 matrix fixture');
    await page.getByRole('button',{name:'complete',exact:true}).click();
    const completedFlake=page.getByRole('combobox').filter({has:page.getByRole('option',{name:'All flakes',exact:true})});
    await completedFlake.selectOption('TASK-488 matrix fixture');
    const latest=page.getByRole('button',{name:'Latest per flake',exact:true});
    if (await latest.getAttribute('aria-pressed') !== 'true') await latest.click();
    await page.getByRole('row').filter({hasText:fixture.matrix.commit_sha.slice(0,12)}).click();
    await drawer.getByRole('button',{name:'Policy matrix',exact:true}).click();
    await drawer.getByRole('button',{name:/^Failing\s+1$/}).click();
    await drawer.getByRole('button',{name:'Name',exact:true}).click();
    await drawer.getByRole('row').filter({hasText:'fixture-host-00'}).getByRole('button',{name:/93\/96.*1 failed/}).click();
    await drawer.getByRole('button',{name:'fixture-host-00',exact:true}).click();
    await drawer.getByRole('button',{name:'Fixture policy 000 · 1.0',exact:true}).click();
    await page.setViewportSize({...originalViewport,height:640});
    try {
      for (const method of ['X','Escape','Backdrop']) {
        const opener=drawer.getByRole('button',{name:'View in compliance evidence · Fixture bundle 2 · v1',exact:true});
        await expect.poll(()=>drawer.locator('.pm-scroll').evaluate(el=>el.scrollHeight>el.clientHeight)).toBe(true);
        await drawer.locator('.pm-scroll').evaluate(el=>{el.scrollTop=80;});
        await opener.focus();
        const scroll=await drawer.locator('.pm-scroll').evaluate(el=>el.scrollTop);
        expect(scroll).toBeGreaterThan(0);
        await opener.press('Enter');
        const receiver=page.getByRole('dialog',{name:'Evaluation check evidence',exact:true});
        await expect(page.getByRole('region',{name:'Exact evaluation evidence'})).toContainText(fixture.matrix.evaluation_attempt_id);
        await dismissDialog(page,receiver,method,'Close evaluation evidence');
        await expect(receiver).toBeHidden();
        expect(new URL(page.url()).pathname).toBe(`/evaluations/${commitId}/attempts/${fixture.matrix.evaluation_attempt_id}`);
        await expect(drawer).toHaveAttribute('data-attempt-number',String(fixture.matrix.attempt_number));
        await expect(completedSearch).toHaveValue('TASK-488 matrix fixture');
        await expect(completedFlake).toHaveValue('TASK-488 matrix fixture');
        await expect(page.getByRole('button',{name:'complete',exact:true})).toHaveClass(/active/);
        await expect(latest).toHaveAttribute('aria-pressed','true');
        await expect(drawer.getByRole('button',{name:/^Controls\s+128$/})).toHaveClass('active');
        await expect(drawer.getByRole('button',{name:'Fixture bundle 1 · v1 ×',exact:true})).toBeVisible();
        await expect(drawer.getByRole('button',{name:'Failing only',exact:true})).toHaveClass(/active-filter/);
        await expect(drawer.getByRole('button',{name:/^Failing\s+1$/})).toHaveClass('active');
        await expect(drawer.getByRole('button',{name:'Name',exact:true})).toHaveClass('active');
        await expect(drawer.getByRole('button',{name:'fixture-host-00',exact:true})).toHaveAttribute('aria-expanded','true');
        await expect(drawer.getByText('[FIXTURE] services.openssh.settings.PermitRootLogin must equal no',{exact:true})).toBeVisible();
        await expect(opener).toBeFocused();
        await expect.poll(()=>drawer.locator('.pm-scroll').evaluate(el=>el.scrollTop)).toBe(scroll);
      }
    } finally {
      await page.setViewportSize(originalViewport);
    }
    // Restore a list-owned drawer for the caller's independent action checks.
    onHistoryPhase(false);
    await page.goto(new URL('/evaluations',evidenceUrl).href);
    // The independent action check below requires the queue's live item.
    await page.getByRole('button',{name:/^Active Queue\b/}).click();
    await page.getByPlaceholder('Search queue…').fill('');
    await page.getByRole('row').filter({hasText:fixture.matrix.commit_sha.slice(0,12)}).click();
    console.log('PASS structured 60-system/128-control matrix overlap, health, drilldown, evidence, live capture/outcome polling and state preservation');
  } finally {
    release?.();
    await page.setViewportSize(originalViewport);
    await page.unroute(endpoint);
    await page.unroute(pinnedEndpoint);
  }
}

module.exports={denseMatrix,runEvalPolicyMatrixWorkflow};
