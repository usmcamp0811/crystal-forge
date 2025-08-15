server.wait_for_unit("postgresql")
server.wait_for_unit("crystal-forge-server.service")
time.sleep(10)

server.log("=== Running SQL View Tests ===")
test_output = server.succeed(
    "psql -U crystal_forge -d crystal_forge -f /etc/crystal-forge-tests.sql"
)
server.log("SQL Test Results:\n" + test_output)
assert "FAIL:" not in test_output, "One or more SQL view tests failed"

# Agent appears in views
view_check = server.succeed(
    """
  psql -U crystal_forge -d crystal_forge -c "
  SELECT hostname, status, status_text
  FROM view_systems_status_table
  WHERE hostname = 'agent';
  "
"""
)
server.log("Agent in status table:\n" + view_check)
assert "agent" in view_check, "Agent hostname not found in view_systems_status_table"

# Simple perf check
start_time = time.time()
server.succeed(
    """
  psql -U crystal_forge -d crystal_forge -c "
  SELECT COUNT(*) FROM view_systems_current_state;
  SELECT COUNT(*) FROM view_systems_status_table;
  SELECT COUNT(*) FROM view_commit_deployment_timeline;
  "
"""
)
query_time = time.time() - start_time
server.log(f"View query performance: {query_time:.2f} seconds")
assert query_time <= 5.0, f"Views are too slow: {query_time:.2f} seconds"

server.log("=== SQL View Tests Completed ===")
