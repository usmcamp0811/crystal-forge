# Access integration check (work in progress)

Snowfall discovers `default.nix` as
`checks.x86_64-linux.integration-access`. The normal check builds **every**
gate in the explicit `scenarios` attribute set. A failed scenario fails the
normal check. Registration is coordinator-owned; adding a script without
registering it is incomplete work.

Currently registered: `lifecycle-local-bootstrap`. It starts a fresh
controller VM with the production embedded-UI server and PostgreSQL, checks
startup migrations and local setup, registers the first administrator in
the browser, signs in, reloads, and reads persisted identity and session
state. Wrong-password rejection must not create a session. Five appended
owner-selected session cases prove missing/mismatched CSRF denial without
session lifecycle writes, valid 204 revocation of only the selected session,
its next GET returning 401 while the retained session returns 200, and an
independent browser signing out through the profile. Browser sign-out requires
logout 204, exact-row invalidation, `/login` after reload, protected GET 401,
and protected navigation returning to `/login`. Six further owner-selected
authorization cases create an Operator and a Viewer through the Admin API,
prove with Admin that each probe payload is valid, and then show that both
roles receive 403 `forbidden` and no protected change for the admin-only user
create, user PATCH and server-configuration (retry policy, classification
banner) mutations. A Viewer browser context finds no administration entry and
the production Access Denied state at `/admin`.

Seven appended Admin-form cases now prove user creation, role edit, cancel,
disable/sign-in rejection, retry settings and classification enable/marking.
Each browser phase reloads and returns to independent SQL before the next
mutation. At `c254b1c54e0f40adfaa9d05b5766cfeffae9bb89`, the normal owner CI
path passed all **23 cases** with a newly executed VM (50.72-second driver,
10.5-second startup reused once). Evidence:
`/nix/store/nv2rpc4szk7525aa9fpb4289nz1hpbqv-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run`.
These are semantic assertions, not visual-parity or complete form coverage.

The historical scenario at `fc930335878279ac426510359c039dfd8961dc13`
failed at server startup. TASK-490.25 fixed shared URI serialization while
preserving the Unix socket; TASK-490.26 passes the selected local/OIDC mode
from the normal module to the production unit. Targeted checks and the normal
server package build pass. After rebase on `origin/dev` `5ae7feab`, the
then-current focused Access gate passed ten cases at
`81d65be745cc9f9777d368900d01fe0f207cb052`; the normal owner aggregate
passed by reusing that run. Evidence:
`/nix/store/i84kvs1lc5nqx0lmsqinwbmvy0ix8mvh-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`.
Actual receipts record 292 migrations and 22 Unix-socket clients. The socket
assertion filters `pg_stat_activity` by configured role/database and
`backend_type = 'client backend'`, requires count > 0, and checks every
`client_addr` is NULL and `client_port` is -1. The
[execution record](../../docs/knowledge/testing/integration-coverage-map.md#current-bootstrap-and-selected-session-proof)
lists exact admin/session receipts and uploaded scenario/browser results and
the signed-out screenshot. The browser result passed at `/login`.
Keep startup, migration, setup-status, browser, persistence, and rejected-login
assertions. Do not add an `AUTH_MODE` test override. The journal also records
TASK-492, a separate Postgres Jobs startup-order defect; it does not fail the
Access gate. TASK-491 resolves the Rust test race; see MR !336.

Failure evidence from tested commit
`fc930335878279ac426510359c039dfd8961dc13`:
`/nix/store/1d5178gxpdv7j4d20dddddr7ik0pq0p3-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`.
`result.json` records failure in `fresh-startup-applies-migrations`;
`journals/controller/generated-config-summary.txt` records the generated
database host and missing auth mode; `journals/controller/crystal-forge.journal`
records `both host and hostaddr are missing`.

## Source-aware administration HTTP boundary (TASK-529.2)

The existing `authorization.py` appends two cases after its original baseline
assertions, in the same fresh controller VM/database:

- `authorization-admin-replacement-preserves-idp-and-classifies-explicit-category`
  uses the existing Admin session and matching CSRF cookie/header. Explicit
  SQL prerequisites give the existing Operator overlapping local/IdP grants,
  unresolved legacy grants and a local-only membership. Role-only PATCH to
  Viewer resolves the role category without classifying legacy memberships.
  Explicit `environments: []` then resolves the membership category. Both writes
  preserve IdP contributions; PATCH and a subsequent Admin list return effective
  Operator and the IdP environment, rather than the requested local authority.
- `authorization-source-union-does-not-admit-operator-admin-mutations` uses the
  original live Operator session with matching CSRF. Self and cross-user Admin
  PATCHes return 403 `forbidden` without business, audit-count or source-ownership
  changes. It restores Operator with empty memberships and removes the injected
  identity/environments before later fragments.

Authority: approved [CF-AGENT-ACCESS-001 v1.1](../../docs/knowledge/security/agent-access-tokens-mcp-remediation-context.md),
sections 4.4 (unchanged human-session boundary) and 5.3 (separate local/managed
ownership), plus the independent administrator/IdP grants decision in TASK-529.2.
Disposition: extend the existing scenario; retain all earlier assertions. The
SQL fixtures use the existing VM-only psql mechanism and schema 0335 source
columns; they supply prerequisites, not the result of the HTTP write.
This is admin HTTP-boundary coverage, not signed real-OIDC callback,
freshness/reconciliation or PAT-admission proof. Current-source runtime results
must come from the parent farm run; the historical passing results above do not
verify this revision.

### Review P1: source-preserving Users modal

Impact authority: the owner's MR345/TASK-529.2 review P1 instruction requires
independent local/IdP ownership and explicit local-edit intent. Affected consumers
are Admin Users create/edit/cancel/disable, environment membership editing, and
non-Admin route access, owned by `lifecycle-local-bootstrap`. Retain the original
identity, permission, persistence, cancellation and reload assertions; update
edit selectors to the opt-in controls and the empty-membership display from
`all` to `None` (empty explicit scope must not be presented as wildcard authority).
Create's existing `all` picker remains the existing empty-list input contract.
These are semantic checks; no visual baseline or layout section is replaced.

`admin-source-grants.{py,cjs}` extends that same lifecycle after authorization
and before Admin forms, with three cases:

- `admin-source-unmarked-grant-roundtrip-rejected-without-writes`: a real Admin
  session plus CSRF receives 400 for unmarked effective-grant round trips,
  unmarked environment/clear requests, explicit false intent, and conflicting
  set/clear role intent. Full source rows, protected state and audits stay equal.
- `admin-source-unrelated-modal-save-preserves-idp-only-grants`: SQL prerequisites
  link the public Operator fixture to one IdP identity, with only IdP Operator
  and one production membership (no local or legacy sources). The real Users
  modal must show effective authority read-only, leave local drafts empty and
  opted out, and submit exactly a public fixture password reset plus disablement.
  Evidence captures the actual PATCH payload and HTTP 200, six-vector
  `grant_sources`, a fresh HTTP/UI reload, full SQL role/membership/identity row
  equality, changed password digest and intended account flag. Audits must contain
  only account/password changes, with no grant classification.
- `admin-source-idp-withdrawal-has-no-accidental-local-fallback`: an explicit SQL
  fixture withdrawal removes only the injected IdP contribution. Any accidental
  local role/membership survives withdrawal and fails the test. SQL and subsequent
  Admin HTTP reads require zero grant rows/vectors and no role. An unrelated
  re-enable and real local login prove the reset public password works with no
  role restored. This is **fixture withdrawal**, not signed IdP callback coverage;
  existing Rust integration tests own callback/reconciliation assertions.

The fragment restores the public Operator password, enabled flag, local Operator
role and empty memberships, then removes the injected identity/environment and
verifies the original public password. Later environment/preferences cases reuse
that actor. Fixture SQL is confined to the fresh scenario VM. No production
test-only endpoint, additional VM, scenario derivation or check is introduced.

`authorization-operator-has-no-usable-admin-mutation-in-ui` also reuses the
Viewer denial CJS contract with an independent Operator context and verifies no
protected mutation. Existing `admin-form-edit-role-persists-and-reloads` now
checks opt-in role intent and exact local source ownership. Existing membership
and restore phases use opt-in environment intent. Unrelated disablement must
submit only `enabled`; all positive grant PATCH fixtures carry
`edit_local_grants: true`.

These additions are **runtime-unverified** until the parent runs the farm gate.
Local verification is limited to Python AST parsing, Node `--check` and whitespace
checking; none is an integration pass.

#### Parent-owned preview browser run

The existing `http://127.0.0.1:8529/admin` preview is baseline-only until the
parent refreshes and verifies the new farm-built server/UI. Do not seed or run
this mutating script against the baseline or a real user's account. Once the
parent verifies a task-isolated preview database and fresh binary, it can prepare
**disposable public fixtures** matching the Python prerequisites and run just
the CJS browser phase with the existing prebuilt `cf-browser` runner:

```bash
cf-browser --spec checks/integration-access/scenarios/fragments/admin-source-grants.cjs \
  --case admin-source-unrelated-save --base-url http://127.0.0.1:8529 \
  --facts /tmp/opencode/admin-source-preview-facts.json \
  --receipts /tmp/opencode/admin-source-preview-receipts.json \
  --out /tmp/opencode/admin-source-preview-browser
```

The facts JSON needs `admin` (disposable local Admin `username`/`password`),
`operator` (`username`/`email`), and `adminSourceGrants` from the scenario `.nix`.
The receipts JSON needs `admin.userId` and `authorization.operator.userId` for
those exact preview fixtures. The Operator must begin active with a local public
password but only IdP-owned Operator and `cf-it-source-production` membership;
the environment must be active and production. Source and environment UUIDs and
the nonsensitive reset password are authored in `adminSourceGrants`. The Admin
must have Admin authority independently of that identity. Never substitute real
account credentials. The spec uses real login/network requests without mocks.

Retain `result.json`, the reloaded modal screenshot and actual PATCH receipt.
This command runs only the browser portion; the parent must perform the Python
fragment's SQL/audit checks, fixture withdrawal and restoration separately for
manual preview proof. The registered farm scenario performs the complete chain
and remains the acceptance gate. This worker has not mutated the preview or run
the browser phase.

## Commands

```bash
# Normal check: every registered scenario.
nix build .#checks.x86_64-linux.integration-access -L

# Focused development: same scenario gate as the normal check.
nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L

# Evidence only: inspect a failed scenario's artifacts. NOT a gate.
nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap.run -L

# Inspect explicit registration.
nix eval .#checks.x86_64-linux.integration-access.scenarios --apply builtins.attrNames
```

The scenario gate's `evidence` link points to its NixOS run output:
`result.json`, `meta.json`, browser artifacts, and journals. The `.run`
target preserves artifacts of an assertion failure, but a package failure
or driver timeout can fail before those files exist. Never use `.run` as a
replacement CI gate. Normal verification does not update visual baselines.

## Missing obligations

Access is not complete. The selected session/CSRF/sign-out proof and the
selected Operator/Viewer admin-denial cases are covered under the owner's
explicit selections (16 cases; see the
[authorization record](../../docs/knowledge/testing/integration-coverage-map.md#access-authorization-cases-task-4902)).
The `admin-forms.{py,cjs}` fragments have seven passing cases at
`c254b1c5`; the original sixteen remain unchanged and first. The expanded map
preserves 79 rows: five with covered subsets, 65 planned and nine decision-blocked.
Environment CRUD/nonempty assignments and specific failed-save states remain
unfinished in TASK-490.3.
TASK-490.2 remains `In Progress`: the cross-environment denial rule and the
closed-registration policy are unimplemented, and each waits for its own
decision. Other Admin routes and visual comparison
are not covered by the selected role-denial cases. OIDC, remaining forms, preferences, notifications, and shell behavior
also remain in the
[coverage map](../../docs/knowledge/testing/integration-coverage-map.md).
GitLab runs this owner automatically and requires it (see below). TASK-490.24
retires the nine legacy suite directories and their manual CI jobs with explicit
owner approval before replacement coverage is complete. Their removal does not
mark obligations covered. The current inventory is one total check, down from
27; the final target is exactly four total `checks.x86_64-linux` attributes.
Delivery, Assurance, and Upgrade owners are absent. Retained Assurance XML
fixtures do not register an owner. No placeholder checks are permitted.
Lower-level assertions run through ordinary component builds, direct
`component-tests` CI commands and existing isolated database workflows. All
fifteen public `verify-*` packages are removed; see the [component catalog](../../docs/knowledge/testing/flake-checks.md#current-inventory-and-commands).

## Composition and runtime prerequisites

`scenarios/lifecycle-local-bootstrap.nix` uses direct `pkgs.writeText` and
`builtins.readFile` concatenation in this order, in one Python namespace and
one controller VM/database:

1. `lifecycle-local-bootstrap.py` completes the five fresh-install assertions
   and establishes `controller`, `admin`, shared helpers and `admin.userId`.
2. `fragments/sessions.py` uses those helpers and that receipt before creating
   additional sessions. `sessions.cjs` is registered as `session-sign-out`.
3. `fragments/authorization.py` creates Operator, Viewer and target users only
   after the fresh-install assertions and session checks. Its controls restore
   the target and installed retry/banner configuration. `authorization.cjs`
   uses its Viewer receipt under `viewer-admin-denied`.
4. `fragments/admin-source-grants.py` reuses the restored public Operator and
   Admin session for the pure-IdP Users modal regression. Its CJS is registered
   as `admin-source-unrelated-save`; SQL assertions, fixture withdrawal and full
   public actor restoration precede subsequent fragments.
5. `fragments/admin-forms.py` requires authorization's `authz_state()` protected
   snapshot for cancel checks, the still-absent design identity `u3`, and restored
   installed configuration, including the disabled banner. `admin-forms.cjs`
   serves the seven registered phases; request/completion receipts select one
   phase at a time, with SQL checks between phases.

Every browser run signs in independently using authored facts and verifies its
identity against same-run receipts. Operator, Viewer and target identities and
probe payloads remain authored in the scenario `.nix`, using stable design IDs.
Module headers document dependencies. Behavior-based filenames express ownership;
no additional registry, VM derivation, owner or flake check is needed.

The first session attempt at `537186835956d7af48a0868289e16364625dd85c`
failed the last UI case due to the coach overlay and unhandled-promise test
mechanics. The passing correction closes the real coach control and uses
`Promise.all` for click/response coordination; assertions remain intact.
Keep runtime-token logs private; the corrected header stays in a VM file.
TASK-490.1 foundation is in `Review`; its documentation validation passed.
Documentation follow-ups do not require another VM run.

## GitLab CI

The `integration-access` job in `.gitlab-ci.yml` runs automatically on merge
requests and `main` and is required. It runs:

```bash
sh packages/ci/integration-owner-build.sh integration-access
```

The script builds the normal owner aggregate with
`sh packages/ci/public-cache-build.sh ".#checks.x86_64-linux.integration-access" -L --show-trace --builders '' --max-jobs 1 --cores 2`.
It never builds a `.run` output as the gate. A `.run` output can build
successfully while its `result.json` records a failed scenario.

Evidence goes to `integration-evidence/integration-access/`, uploaded with
`artifacts: when: always`:

- `nix-build.log` — the complete build output.
- `<scenario>/` — real files, not Nix store symlinks: `result.json`,
  `meta.json`, `browser/` (results, screenshots, console logs), and
  `journals/`.

After a successful build, the script copies `result/<scenario>/evidence/`. If
the build succeeded but evidence is missing, the job fails. After a failed
build, the `result` link does not exist. The script then resolves each
registered scenario's `.run.outPath` through evaluation and copies only outputs
that already exist, including the last listed scenario (the listing has no final
newline). A copy failure is reported and counted; it does not replace the build
status, and the remaining scenarios are still collected. The script never builds
a `.run` derivation, because a crash or timeout could run the VM again and
replace the original attempt's evidence. After a failed build it always exits
with the original build status. After a successful build, a missing or
uncopyable evidence directory fails the job. The scenario names come from the
owner's own `scenarios` attribute set; the script keeps no second list.

Run the same script locally from the repository root. It removes a stale
`result` link and writes `integration-evidence/`, which Git ignores.

Shared mechanics live under `lib/integration-test/`, exposed through
`lib.crystal-forge.makeIntegrationTestHarness { inherit pkgs inputs; }`.
Read the [maintenance contract](../../docs/knowledge/testing/integration-test-contract.md)
before changing expectations. Application and design sources are read-only
for this overhaul.
