# Only this scenario's fresh VM/database is seeded. Reporting, authorization,
# persistence and browser reads use the real production server and signed agent
# fixture. Preexisting build/cache rows are explicitly not workflow evidence.
admin = FACTS["admin"]
start_all()


def delivery_api(method, path, body=None, expected=200):
    status, response = http(controller, method, path, body,
                            session="delivery-admin", csrf=method != "GET")
    expect_equal(status, expected, method + " " + path + ": " + json.dumps(response))
    return response


def literal(value):
    return "'" + str(value).replace("'", "''") + "'"


def fixture_sql(query):
    _vm_write(controller, "/tmp/cf-it/reporting-fixture.sql", query)
    return controller.succeed("runuser -u postgres -- psql -X -q -v ON_ERROR_STOP=1 "
        "-d crystal_forge -f - < /tmp/cf-it/reporting-fixture.sql")


with case("isolated-reporting-administrator-and-preexisting-artifact"):
    controller.wait_for_unit("crystal-forge-server.service", timeout=180)
    controller.wait_for_open_port(3000)
    status, registered = http(controller, "POST", "/api/auth/local/register", {
        "username": admin["username"], "email": admin["email"], "password": admin["password"],
        "first_name": admin["firstName"], "last_name": admin["lastName"],
    })
    expect_equal(status, 201, "independent administrator registration")
    receipt("delivery.adminId", registered["user_id"])
    status, _ = http(controller, "POST", "/api/auth/local/login", {
        "username": admin["username"], "password": admin["password"],
    }, session="delivery-admin")
    expect_equal(status, 200, "independent administrator login")
    delivery_api("POST", "/api/v1/environments", {
        "name": "delivery-proof", "description": "Isolated failure reporting fixture",
        "color_hex": "#2563eb", "is_active": True, "default_policy": "manual",
    }, expected=201)
    # No fetchable upstream: the seed is intentionally preexisting, not a
    # substitute for the sibling lifecycle's real evaluation/publication proof.
    flake = delivery_api("POST", "/api/v1/flakes", {
        "name": FACTS["flake"]["name"], "repo_url": "http://127.0.0.1:9/preexisting.git",
        "branch": "main", "build_scope": "cf_systems_only",
    }, expected=201)
    receipt("delivery.flakeId", flake["id"])
    receipt("delivery.sourceRevision", FACTS["sourceRevision"])
    receipt("delivery.outputPath", FACTS["outputPath"])
    fixture_sql("WITH c AS (INSERT INTO commits (flake_id,git_commit_hash,commit_timestamp,evaluation_status) VALUES (" +
        str(flake["id"]) + "," + literal(FACTS["sourceRevision"]) + ",NOW(),'complete') RETURNING id), "
        "d AS (INSERT INTO derivations (commit_id,derivation_type,derivation_name,derivation_path,"
        "store_path,expected_store_path,status_id,completed_at,cf_agent_enabled,policy_requirements_met) "
        "SELECT id,'nixos'," + literal(FACTS["target"]) + "," + literal(FACTS["outputPath"] + ".drv") + "," +
        literal(FACTS["outputPath"]) + "," + literal(FACTS["outputPath"]) + ",10,NOW(),TRUE,TRUE FROM c RETURNING id) "
        "INSERT INTO cache_push_jobs (derivation_id,store_path,status,completed_at,cache_destination) SELECT id," +
        literal(FACTS["outputPath"]) + ",'completed',NOW(),'preexisting-fixture' FROM d;")
    receipt("failure.preexistingArtifactFixture", True)
