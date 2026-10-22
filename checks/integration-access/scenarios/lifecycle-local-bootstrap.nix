# access.lifecycle.local-bootstrap (registered in ../default.nix).
#
# Actor/scope: the first operator of a fresh installation, local auth mode.
# Starting state: new controller VM, empty PostgreSQL, normal module config
#   with services.crystal-forge.server.auth_mode = "local" (module default).
# Action: open the production UI, register the first administrator, sign in,
#   and reload.
# Required outcome: migrations apply at startup; the fresh server reports
#   local mode and requires setup; the first user persists with exactly the
#   Admin role; one active session exists after sign-in and survives reload.
# Negative condition: a wrong password is rejected with 401 and creates no
#   session or session cookie.
# Appended session cases: two named sessions prove missing/mismatched CSRF
#   rejection, valid revocation of only the selected row, and rejection of
#   that session's next operation. An independent browser signs in and out;
#   its exact persisted session is invalidated. These cases run only after
#   the five bootstrap cases.
# Appended authorization cases: Operator
#   and Viewer actors, created through the Admin API, are denied the
#   admin-only user create, user PATCH and server-configuration mutations with
#   403 and no protected state change. A Viewer browser context finds no
#   administration entry and the production "Access Denied" state at /admin.
#   Authority: the approved design's role catalogue (Admin: "Manage users &
#   OIDC", "Edit server config") and its Server Management page ("Admin-only").
#   Not covered here: cross-environment scope and closed registration.
# Appended Admin forms: creation, edit, cancel, disable/login
#   rejection, retry settings and classification save/reload. Each browser
#   phase returns to SQL verification before the next mutation. Environment
#   CRUD, outage draft states and visual parity remain separate obligations.
# Authority: the owner-approved TASK-490.1 requirement is a fresh first-admin
#   local-auth bootstrap using the normal NixOS module with
#   services.crystal-forge.server.auth_mode = "local". The operations/auth
#   and security/API-auth docs describe current implementation; treat them as
#   evidence, not independent approval of behavior.
# Design reference: none. The approved design handoff has no Login or
#   Register component, so this scenario makes semantic assertions only
#   (Blocked: design gap for visual comparison of /register and /login).
{
  harness,
  owner,
  name,
}: let
  inherit (harness) lib;
  migrationDir = "${harness.inputs.self}/packages/default/crates/cf-server/migrations";
  # Independent expectation: the number of forward migrations in the source
  # tree that the production server embeds with sqlx::migrate!.
  expectedMigrations = builtins.length (builtins.filter (
    n:
      builtins.match "[0-9]+_.*\\.sql" n != null && !(lib.hasSuffix ".down.sql" n)
  ) (builtins.attrNames (builtins.readDir migrationDir)));
in
  harness.mkScenario {
    inherit owner name;
    contracts = ["access.lifecycle"];
    nodes.controller = harness.roles.controller {browser = true;};
    facts = {
      authMode = "local";
      database = { name = "crystal_forge"; user = "crystal_forge"; };
      inherit expectedMigrations;
      admin = harness.facts.localUserFromDesign {
        id = "u1";
        username = "mreyes";
        password = "cf-it-access-bootstrap-1";
      };
      # Wire value of the Admin role in /api/auth/whoami (`Role`, PascalCase).
      adminRoleWire = "Admin";
      # Database value of the Admin role (`auth_role` enum).
      adminRoleDb = "admin";

      # Authorization actors. Identity, display name, email and
      # role come from the approved design fixture by stable ID (u2 is an
      # Operator, u4 a Viewer, u7 a Viewer that serves as the protected
      # target). The server derives a local username from the email's local
      # part, so the authored username below is a checked expectation, not
      # an input. `roleWire` is the PascalCase role in /api/auth/whoami.
      operator = harness.facts.localUserFromDesign {
        id = "u2";
        username = "jpark";
        password = "cf-it-access-operator-1";
      } // { roleWire = "Operator"; };
      viewer = harness.facts.localUserFromDesign {
        id = "u4";
        username = "kthomas";
        password = "cf-it-access-viewer-1";
      } // { roleWire = "Viewer"; };
      target = harness.facts.localUserFromDesign {
        id = "u7";
        username = "audit";
        password = "cf-it-access-target-1";
      } // { roleWire = "Viewer"; };

      # A new design identity: the earlier role-denial cases already created
      # u2/u4/u7. Form cases must observe the creation of this account themselves.
      adminFormUser = harness.facts.localUserFromDesign {
        id = "u3";
        username = "dchen";
        password = "cf-it-admin-form-1";
      };
      adminForms = {
        initialRoleWire = "Operator";
        initialRoleDb = "operator";
        editedRoleWire = "Viewer";
        editedRoleDb = "viewer";
        cancelEmail = "cancelled-admin-form@acme.io";
        retryPolicy = {
          max_build_retries = 4;
          max_evaluation_retries = 2;
          backoff_seconds = 60;
          transient_only = false;
        };
        classification = {
          enabled = true;
          level = "CUI";
          custom_text = "CF-IT ADMIN FORM";
        };
      };

      environmentForms = let
        create = {
          name = "cf-it-form-tier";
          description = "Environment form creation probe";
          colorName = "blue";
          color_hex = "#2563EB";
          default_policy = "manual";
          auto_sync = true;
          requires_approval = true;
          is_production = false;
        };
        edit = create // {
          name = "cf-it-form-tier-edited";
          description = "Environment form edited probe";
          colorName = "emerald";
          color_hex = "#059669";
          default_policy = "pinned";
          auto_sync = false;
          requires_approval = false;
          is_production = true;
        };
      in {
        inherit create edit;
        # Canonical enabled optional policy from migration 0081. The legacy
        # agent policy is a global invariant, not an environment assignment.
        optionalPolicy = {
          id = "00000000-0000-0000-0000-000000000003";
          name = "Custom Check";
          policy_type = "custom_check";
        };
        invalidPolicyIds = [
          "00000000-0000-0000-0000-000000000001"
          "ffffffff-ffff-4fff-8fff-ffffffffffff"
        ];
        cancelName = "cf-it-form-cancelled-create";
        cancel = create // {
          name = "cf-it-form-cancelled-edit";
          description = "Cancelled environment draft";
          colorName = "red";
          color_hex = "#DC2626";
        };
        duplicate = edit // {
          description = "Retained duplicate environment draft";
          colorName = "violet";
          color_hex = "#7C3AED";
          default_policy = "auto_latest";
          auto_sync = true;
          requires_approval = true;
          is_production = false;
        };
      };

      # Authored changed targets for u1; u2 retains its same-run UI baseline.
      # Email delivery is unavailable in this local-auth controller.
      preferences = {
        appearance = {
          theme = "light";
          density = "compact";
          sidebar_collapsed = true;
          default_systems_view = "table";
        };
        notifications = {
          deploy_failures = false;
          build_failures = false;
          critical_cves = false;
          policy_violations = false;
          heartbeat_lost = true;
          weekly_digest = false;
          delivery_channel = "in_app";
        };
      };

      # Authored request payloads for the admin-only operations. Each denied
      # request uses a payload that the Admin control case first proves is
      # accepted, so a 403 can only be caused by the actor's role.
      authorization = {
        # Role escalation by account creation. One address per actor, and one
        # for the Admin control, so a wrongly accepted request is attributable.
        createEmails = {
          control = "escalation-control@acme.io";
          operator = "escalation-by-operator@acme.io";
          viewer = "escalation-by-viewer@acme.io";
        };
        createProbe = {
          display_name = "Escalation Probe";
          password = "cf-it-access-escalation-1";
          role = "Admin";
          environments = [];
        };
        # Role escalation and disablement of an existing account.
        patchProbe = { role = "Admin"; enabled = false; };
        patchRestore = { role = "Viewer"; enabled = true; };
        # Server configuration values that differ from the installed defaults.
        retryPolicy = {
          max_build_retries = 3;
          max_evaluation_retries = 2;
          backoff_seconds = 60;
          transient_only = false;
        };
        classification = {
          enabled = true;
          level = "CUI";
          custom_text = "CF-IT AUTHORIZATION PROBE";
        };
      };
    };
    # One controller/database and one Python namespace, in dependency order:
    # bootstrap establishes controller/admin/helpers and admin.userId before
    # sessions creates more sessions. Authorization creates actors only after
    # fresh-install assertions and exports authz_state() for form cancel checks.
    # Forms require u3 still absent and authorization's retry/banner controls
    # restored to installed values (banner disabled). Keep direct concatenation.
    # Preserve all 23 existing cases before shell/preferences. Shell leaves u1's
    # sidebar collapsed; preferences then changes u1 and snapshots active u2,
    # not u3 (disabled by Admin forms). Each phase uses independent browser state.
    # TASK-490.4 visual parity is explicitly deferred by the owner: no comparison
    # or passing visual placeholder is composed. Crop, fixed justified threshold,
    # helper execution and retained reference/actual/diff evidence remain unverified.
    script = harness.pkgs.writeText "access-local-lifecycle.py" (
      builtins.readFile ./lifecycle-local-bootstrap.py
      + "\n"
      + builtins.readFile ./fragments/sessions.py
      + "\n"
      + builtins.readFile ./fragments/authorization.py
      + "\n"
      + builtins.readFile ./fragments/admin-forms.py
      + "\n"
      + builtins.readFile ./fragments/environment-forms.py
      + "\n"
      + builtins.readFile ./fragments/preferences.py
      + "\n"
      + builtins.readFile ./fragments/shell-navigation.py
    );
    browserSpecs = {
      bootstrap-sign-in = ./lifecycle-local-bootstrap.cjs;
      session-sign-out = ./fragments/sessions.cjs;
      viewer-admin-denied = ./fragments/authorization.cjs;
      # Distinct artifacts and contexts, not additional VM scenarios. Each
      # phase returns to Python for authoritative SQL before the next mutation.
      admin-form-create = ./fragments/admin-forms.cjs;
      admin-form-edit = ./fragments/admin-forms.cjs;
      admin-form-cancel = ./fragments/admin-forms.cjs;
      admin-form-disable = ./fragments/admin-forms.cjs;
      admin-form-retry = ./fragments/admin-forms.cjs;
      admin-form-classification-enable = ./fragments/admin-forms.cjs;
      admin-form-classification = ./fragments/admin-forms.cjs;
      environment-form-create = ./fragments/environment-forms.cjs;
      environment-form-edit = ./fragments/environment-forms.cjs;
      environment-form-optional = ./fragments/environment-forms.cjs;
      environment-form-cancel = ./fragments/environment-forms.cjs;
      environment-form-duplicate = ./fragments/environment-forms.cjs;
      environment-form-membership = ./fragments/environment-forms.cjs;
      environment-form-membership-restore = ./fragments/environment-forms.cjs;
      environment-form-delete = ./fragments/environment-forms.cjs;
      shell-protected = ./fragments/shell-navigation.cjs;
      shell-routes = ./fragments/shell-navigation.cjs;
      shell-keyboard = ./fragments/shell-navigation.cjs;
      shell-mobile = ./fragments/shell-navigation.cjs;
      shell-sidebarPersistence = ./fragments/shell-navigation.cjs;
      shell-unknown = ./fragments/shell-navigation.cjs;
      preferences-baseline-a = ./fragments/preferences.cjs;
      preferences-baseline-b = ./fragments/preferences.cjs;
      preferences-appearance = ./fragments/preferences.cjs;
      preferences-appearance-read-failure = ./fragments/preferences.cjs;
      preferences-appearance-save-failure = ./fragments/preferences.cjs;
      preferences-notifications = ./fragments/preferences.cjs;
      preferences-notification-read-failure = ./fragments/preferences.cjs;
      preferences-notification-save-failure = ./fragments/preferences.cjs;
      preferences-notification-loading = ./fragments/preferences.cjs;
      preferences-verify-b = ./fragments/preferences.cjs;
    };
    timeout = 900;
  }
