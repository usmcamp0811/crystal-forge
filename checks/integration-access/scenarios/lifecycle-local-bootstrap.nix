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
    };
    script = ./lifecycle-local-bootstrap.py;
    browserSpecs.bootstrap-sign-in = ./lifecycle-local-bootstrap.cjs;
    timeout = 900;
  }
