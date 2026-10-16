/**
 * Exercises structured matrix presentation using a deterministic API fixture.
 * The backend's actual resolver/capture path has separate migration-backed tests.
 * These responses are browser-local; they never rewrite persisted evaluations.
 */
const { expect } = require('@playwright/test');

function id(index) {
  return `04880488-0000-4000-8000-${index.toString(16).padStart(12, '0')}`;
}

function denseMatrix(commitId) {
  const columns = Array.from({length: 128}, (_, i) => ({
    policy_version_id: id(i + 1), policy_id: id(i + 1001),
    name: `Fixture policy ${String(i).padStart(3, '0')}`, version: '1.0',
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
    matrix:{evaluation_attempt_id:id(9001), attempt_number:1, provenance:'captured',
      columns,bundles,empty_bundle_metadata_complete:true,rows},
  };
}

async function runEvalPolicyMatrixWorkflow(page, commitId, screenshot) {
  const fixture = denseMatrix(commitId);
  const endpoint = `**/commits/${commitId}/eval/policy-matrix*`;
  // A request can beat plan capture. A live tab must discover the later capture
  // without requiring the user to close or switch tabs.
  let response={...fixture,matrix:null};
  let fail=false;
  let pending=null;
  let release=null;
  await page.route(endpoint, async route => {
    if(pending) await pending;
    if(fail) await route.fulfill({status:503,body:'Fixture matrix unavailable'});
    else await route.fulfill({json:response});
  });
  try {
    const drawer=page.getByRole('dialog',{name:'Evaluation detail',exact:true});
    await drawer.getByRole('button',{name:'Policy matrix',exact:true}).click();
    await expect(drawer.getByText(/Historical bundle provenance unavailable/)).toBeVisible();
    response=fixture;
    await expect(drawer.getByRole('button',{name:/^Bundles\s+6$/})).toBeVisible({timeout:12000});
    await expect(drawer.getByText('128 controls · 32 shared across bundles',{exact:true})).toBeVisible();
    await expect(drawer.getByRole('button',{name:/^Clean\s+56$/})).toBeVisible();
    await expect(drawer.getByRole('button',{name:/^Unavailable \/ N\/A\s+2$/})).toBeVisible();
    await expect(drawer.getByRole('columnheader')).toHaveCount(8);
    await expect(drawer.getByRole('cell')).toHaveCount(60 * 8);
    await screenshot('eval-matrix-bundles');

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
    await row.getByRole('button',{name:'fixture-host-00',exact:true}).click();
    await expect(drawer.getByText('[FIXTURE] services.openssh.settings.PermitRootLogin must equal no',{exact:true})).toBeVisible();
    await screenshot('eval-matrix-controls-detail');
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
    fail=true;
    await reopen();
    await expect(drawer.getByText('Failed to load policy matrix',{exact:true})).toBeVisible();
    fail=false;
    response={...fixture,matrix:{...fixture.matrix,columns:[],bundles:[],rows:[]}};
    pending=new Promise(resolve=>{release=resolve;});
    await reopen();
    await expect(drawer.getByText('Loading policy matrix...',{exact:true})).toBeVisible();
    release();
    pending=null;
    await expect(drawer.getByText(/No eligible control columns were captured/)).toBeVisible();
    await expect(drawer.getByText('No planned systems match this filter.',{exact:true})).toBeVisible();
    response={...fixture,matrix:null};
    await reopen();
    await expect(drawer.getByText(/Historical bundle provenance unavailable/)).toBeVisible();
    await expect(drawer.getByRole('button',{name:/^Bundles\s+6$/})).toHaveCount(0);
    console.log('PASS structured 60-system/128-control matrix overlap, health, drilldown, evidence, live capture/outcome polling and state preservation');
  } finally {
    release?.();
    await page.unroute(endpoint);
  }
}

module.exports={denseMatrix,runEvalPolicyMatrixWorkflow};
