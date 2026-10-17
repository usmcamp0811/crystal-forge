/**
 * Exercises structured matrix presentation using a deterministic API fixture.
 * The backend's actual resolver/capture path has separate migration-backed tests.
 * These responses are browser-local; they never rewrite persisted evaluations.
 */
const { expect } = require('@playwright/test');

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
    name: `Fixture bundle ${i+1}`, version: 'v1', framework: 'Fixture framework',
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

async function runEvalPolicyMatrixWorkflow(page, commitId, screenshot, onSnapshot) {
  const fixture = denseMatrix(commitId);
  const originalDrawer=page.getByRole('dialog',{name:'Evaluation detail',exact:true});
  fixture.matrix.commit_sha=await originalDrawer.getAttribute('data-commit-sha');
  fixture.matrix.attempt_number=Number(await originalDrawer.getAttribute('data-attempt-number'));
  fixture.matrix.mock_evidence=true;
  const endpoint = `**/commits/${commitId}/eval/policy-matrix*`;
  const pinnedEndpoint = `**/commits/${commitId}/eval/attempts/${fixture.matrix.evaluation_attempt_id}/policy-matrix*`;
  // A captured plan can precede discovery. A live tab must discover inventory
  // without requiring the user to close or switch tabs.
  let response={...fixture,matrix:{...fixture.matrix,inventory:null,configuration_summary:null}};
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
    response=fixture;
    await expect(drawer.getByRole('button',{name:/^Bundles\s+6$/})).toBeVisible({timeout:12000});
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
    await expect(drawer.locator('.pm-controls-line')).toHaveCount(2);
    await capture('eval-matrix-bundles');
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
    await expect(drawer.getByText('No systems match this filter.',{exact:true})).toBeVisible();
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

    // Missing captured contributions fail closed without a deployed fallback.
    pinnedResponse={...fixture,matrix:{...fixture.matrix,rows:fixture.matrix.rows.map((r,index)=>index===0 ? {...r,policies:r.policies.map((p,i)=>i===0 ? {...p,memberships:[]} : p)} : r)}};
    await drawer.getByRole('button',{name:'View in compliance evidence · Fixture bundle 2 · v1',exact:true}).click();
    await expect(page.getByRole('dialog',{name:'Evaluation check evidence',exact:true}).getByRole('alert')).toContainText('exact captured context or membership does not match');
    await expect(page.getByRole('region',{name:'Exact evaluation evidence'})).toHaveCount(0);
    pinnedResponse=null;
    await page.getByRole('button',{name:'Back to Evaluation',exact:true}).click();
    await expect(drawer.getByText('[FIXTURE] services.openssh.settings.PermitRootLogin must equal no',{exact:true})).toBeVisible();
    await expect(drawer.getByRole('button',{name:'Log unavailable for retained attempt',exact:true})).toBeDisabled();
    await drawer.getByRole('button',{name:'Open latest evaluations',exact:true}).click();
    await page.getByRole('row').filter({hasText:fixture.matrix.commit_sha.slice(0,12)}).click();
    await drawer.getByRole('button',{name:'Policy matrix',exact:true}).click();
    await drawer.getByRole('button',{name:'Name',exact:true}).click();

    // The API, not this browser, reconciles discovered unregistered configs.
    // Require its exact appended row and keep unavailable applicability unknown.
    const unmanaged={system_id:null,system_name:'fixture-unmanaged-config',
      configuration_name:'fixture-unmanaged-config',resolution:'unavailable',
      evaluation_digest:null,assigned_bundles:[],policies:[],
      cells:fixture.matrix.columns.map(()=> 'unknown'),details:fixture.matrix.columns.map(()=>null)};
    const reconciledRows=[...fixture.matrix.rows,unmanaged];
    response={...fixture,matrix:{...fixture.matrix,rows:reconciledRows,
      ...inventoryFixture(reconciledRows,{passed:58,failed:1,evaluation_failed:0,pending:1,unavailable:1,excluded:0})}};
    await reopen();
    await drawer.getByRole('button',{name:/^All\s+61$/}).click();
    await assertNames([...expectedNames,'fixture-unmanaged-config']);
    await assertHeaders();
    const unmanagedRow=drawer.getByRole('row').filter({has:page.getByRole('button',{name:'fixture-unmanaged-config',exact:true})});
    await expect(unmanagedRow.getByRole('cell').nth(1)).toHaveText('Unknown');
    await expect(unmanagedRow.getByText('N/A',{exact:true})).toHaveCount(0);
    await unmanagedRow.getByRole('button',{name:'fixture-unmanaged-config',exact:true}).click();
    await expect(drawer.getByRole('button',{name:'Open system',exact:true})).toBeDisabled();
    await expect(drawer.getByRole('button',{name:/^View in compliance evidence/})).toHaveCount(0);
    await unmanagedRow.getByRole('button',{name:'fixture-unmanaged-config',exact:true}).click();

    // Primary coverage is the unique union, while shared host contexts remain
    // separate rows. One failing context classifies the whole config as failing.
    const unionRows=fixture.matrix.rows.slice(4,7).map((r,i)=>({...r,
      cells:r.cells.map((_,c)=>i===2 && c===0 ? 'fail' : 'pass'),details:r.details.map(()=>null)}));
    const shared={...unionRows[2],system_id:id(6999),system_name:'fixture-shared-host',cells:unionRows[2].cells.map(()=> 'pass')};
    const discoveredOnly={...unmanaged,system_name:'fixture-discovery-only',configuration_name:'fixture-discovery-only'};
    response={...fixture,matrix:{...fixture.matrix,rows:[...unionRows,shared,discoveredOnly],
      inventory:{execution_mode:'mock',configurations:['fixture-config-5','fixture-config-6','fixture-discovery-only'],
        selected_configurations:['fixture-config-6','fixture-discovery-only']},
      configuration_summary:{total:4,passed:0,failed:1,evaluation_failed:0,pending:1,unavailable:1,excluded:1}}};
    await reopen();
    await drawer.getByRole('button',{name:/^All\s+4$/}).click();
    await assertNames(['fixture-host-04','fixture-host-05','fixture-host-06','fixture-shared-host','fixture-discovery-only']);
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
    await expect(drawer.getByRole('button',{name:/^Unavailable \/ N\/A\s+3$/})).toBeVisible();

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
      await expect(drawer.getByRole('button',{name:/^(Bundles|Controls)\s+\d+$/})).toHaveCount(0);
      await expect(drawer.getByRole('button',{name:'Failing only',exact:true})).toHaveCount(0);
      await expect(drawer.locator('.pm-controls-line')).toHaveCount(1);
      const filterLine=drawer.locator('.pm-controls-line').filter({has:page.getByRole('button',{name:/^All\s+12$/})});
      await expect(filterLine.getByRole('button',{name:'Name',exact:true})).toBeVisible();
      await filterLine.getByRole('button',{name:'Name',exact:true}).click();
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
      await expect(retained).toHaveAttribute('data-attempt-number',String(fixture.matrix.attempt_number));
      await expect(retained).toHaveAttribute('data-attempt-id',fixture.matrix.evaluation_attempt_id);
      const retainedStat=label=>retained.locator('.ed-stat').filter({has:freshPage.getByText(label,{exact:true})}).locator('.ed-stat-val');
      await expect(retainedStat('Systems')).toHaveText('60');
      await expect(retainedStat('Policy')).toHaveText(/^58\s*\/\s*1\s*1 unavailable$/);
      await expect(retained.getByRole('tab',{name:'Log unavailable for retained attempt',exact:true})).toBeDisabled();
      await expect(retained.getByRole('tab',{name:'Dependency graph unavailable',exact:true})).toBeDisabled();
      await expect(retained.getByRole('button',{name:/^All\s+60$/})).toHaveClass('active');
      expect(freshRequests.every(url=>new URL(url).pathname===new URL(pinnedRequests[0]).pathname)).toBe(true);
    } finally {
      await freshContext.close();
    }
    // Restore a list-owned drawer for the caller's independent action checks.
    await page.goto(new URL('/evaluations',evidenceUrl).href);
    await page.getByRole('row').filter({hasText:fixture.matrix.commit_sha.slice(0,12)}).click();
    console.log('PASS structured 60-system/128-control matrix overlap, health, drilldown, evidence, live capture/outcome polling and state preservation');
  } finally {
    release?.();
    await page.unroute(endpoint);
    await page.unroute(pinnedEndpoint);
  }
}

module.exports={denseMatrix,runEvalPolicyMatrixWorkflow};
