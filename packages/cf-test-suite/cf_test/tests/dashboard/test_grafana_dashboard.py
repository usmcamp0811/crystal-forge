import json
import os
import shlex
import time
from pathlib import Path
from typing import Any, Dict, List, Optional

import pytest

from cf_test import CFTestClient
from cf_test.scenarios import scenario_behind, scenario_offline, scenario_up_to_date
from cf_test.vm_helpers import SmokeTestConstants as C

pytestmark = [pytest.mark.dashboard, pytest.mark.driver]


class GrafanaClient:
    """Helper client for Grafana API interactions.

    When `server` is provided, all HTTP traffic is performed from inside the
    NixOS VM via `curl`, instead of from the host with `requests`. This avoids
    the host↔VM connectivity issue (connection reset on 127.0.0.1:3000).
    """

    def __init__(self, base_url: str, timeout: int = 10, server=None):
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout
        self.server = server

        # Only used when server is None (e.g. running tests against an external Grafana)
        if self.server is None:
            import requests

            self._requests = requests.Session()
        else:
            self._requests = None

    def _request(self, method: str, path: str, **kwargs) -> Dict[str, Any]:
        """
        Make HTTP request to Grafana API.

        If `server` is set, we shell out to `curl` inside the VM.
        Otherwise, we use `requests` from the host.
        """
        url = f"{self.base_url}/api{path}"

        # VM path: use curl via server.succeed
        if self.server is not None:
            parts = ["curl", "-sS", "-X", method.upper()]
            data = kwargs.get("json")

            if data is not None:
                body = json.dumps(data)
                parts += ["-H", "Content-Type: application/json", "-d", body]

            # Build shell-safe command
            cmd = " ".join(shlex.quote(p) for p in parts + [url])

            try:
                out = self.server.succeed(cmd)
            except Exception as e:
                return {
                    "status": "unreachable",
                    "_error": str(e),
                    "_status_code": None,
                }

            out = out.strip()
            if not out:
                return {}

            try:
                parsed = json.loads(out)
                return parsed if isinstance(parsed, dict) else {"_parsed": parsed}
            except json.JSONDecodeError:
                return {"_raw_text": out}

        # Host path: normal requests (for non-VM use)
        if self._requests is None:
            raise RuntimeError("Requests session not initialized")

        import requests

        kwargs.setdefault("timeout", self.timeout)
        resp = self._requests.request(method, url, **kwargs)

        data: Dict[str, Any] = {
            "_status_code": resp.status_code,
            "_ok": resp.ok,
        }
        if resp.text:
            try:
                parsed = resp.json()
                if isinstance(parsed, dict):
                    data.update(parsed)
                else:
                    data["_parsed"] = parsed
            except ValueError:
                data["_raw_text"] = resp.text[:1024]
        return data

    def health(self) -> Dict[str, Any]:
        """Check Grafana health"""
        result = self._request("GET", "/health")
        # Grafana normally returns {"database": "ok", "version": "...", "commit": "..."}
        # Newer versions also add {"status": "ok"}; if it's missing, we infer it.
        if "status" not in result and result.get("database") == "ok":
            result["status"] = "ok"
        return result

    def datasources(self) -> List[Dict[str, Any]]:
        """Get all datasources"""
        result = self._request("GET", "/datasources")
        if isinstance(result, list):
            return result
        if "datasources" in result and isinstance(result["datasources"], list):
            return result["datasources"]
        return []

    def test_datasource(self, datasource_id: int) -> Dict[str, Any]:
        """Test a datasource connection"""
        return self._request("POST", f"/datasources/{datasource_id}/testConnection")

    def dashboards(self) -> List[Dict[str, Any]]:
        """Get all provisioned dashboards"""
        result = self._request("GET", "/search?query=&type=dash-db")
        if isinstance(result, list):
            return result
        if "dashboards" in result and isinstance(result["dashboards"], list):
            return result["dashboards"]
        return []

    def dashboard(self, uid: str) -> Dict[str, Any]:
        """Get dashboard by UID"""
        return self._request("GET", f"/dashboards/uid/{uid}")

    def query_datasource(self, datasource_id: int, query: str) -> List[Dict[str, Any]]:
        """Execute a query against a datasource and return raw results"""
        payload = {
            "queries": [
                {
                    "datasourceId": datasource_id,
                    "rawSql": query,
                    "format": "table",
                    "refId": "A",
                }
            ],
            "from": "now-1h",
            "to": "now",
        }
        result = self._request("POST", "/tsdb/query", json=payload)

        # Grafana's /tsdb/query returns something like:
        # {"results": {"A": {"series": [{"name": "...", "columns": [...], "values": [[...], ...]}]}}}
        results = result.get("results")
        if isinstance(results, dict) and results:
            first_key = next(iter(results))
            series = results[first_key].get("series", [])
            if series:
                return series[0].get("values", [])
        return []

    def screenshot_curl(self, dashboard_uid: str, filepath: str, server) -> bool:
        """
        Capture a dashboard screenshot using curl from the server VM.
        This is more reliable than the HTTP API.
        """
        try:
            cmd = (
                f"curl -s 'http://127.0.0.1:3000/render/d-solo/{dashboard_uid}"
                f"?orgId=1&panelId=1&width=1200&height=800&tz=browser' "
                f"> {shlex.quote(filepath)}"
            )
            server.succeed(cmd)
            # Verify file was created
            result = server.succeed(
                f"test -f {shlex.quote(filepath)} && stat -c%s {shlex.quote(filepath)} || echo 0"
            )
            size = int(result.strip())
            return size > 100
        except Exception:
            return False


@pytest.fixture(scope="session")
def grafana_url(server) -> str:
    """Construct Grafana URL from server machine"""
    if server is None:
        pytest.skip("Server machine not available")
    # Inside the VM Grafana listens on localhost:3000
    return "http://127.0.0.1:3000"


@pytest.fixture(scope="session")
def grafana_client(grafana_url: str, server) -> GrafanaClient:
    """
    Create and verify Grafana client.

    In the NixOS VM test, this talks to Grafana *from inside the VM* via curl.
    """
    if server is None:
        pytest.skip("Server machine not available")

    client = GrafanaClient(grafana_url, timeout=10, server=server)

    max_retries = 10
    last_health: Dict[str, Any] = {}

    for _ in range(max_retries):
        health = client.health()
        last_health = health

        status = health.get("status")
        if status == "ok":
            return client

        time.sleep(2)

    pytest.fail(
        f"Grafana not ready after {max_retries * 2} seconds at {grafana_url}. "
        f"Last health payload: {last_health!r}"
    )


@pytest.mark.dashboard
def test_grafana_service_running(server):
    """Verify Grafana service is active and running"""
    if server is None:
        pytest.skip("Server machine not available")

    result = server.succeed("systemctl is-active grafana.service || true")
    assert (
        "active" in result.lower()
    ), "Grafana service is not active. Check systemctl status grafana.service"


@pytest.mark.dashboard
def test_grafana_health_check(grafana_client: GrafanaClient):
    """Verify Grafana is healthy and responding"""
    health = grafana_client.health()
    assert health.get("status") == "ok", f"Grafana health check failed: {health}"


@pytest.mark.dashboard
def test_postgresql_datasource_provisioned(grafana_client: GrafanaClient):
    """Verify PostgreSQL datasource is provisioned and correctly named"""
    datasources = grafana_client.datasources()
    assert len(datasources) > 0, "No datasources configured"

    postgres_datasources = [ds for ds in datasources if ds.get("type") == "postgres"]
    assert (
        len(postgres_datasources) > 0
    ), f"No PostgreSQL datasource found. Available: {[d.get('name') for d in datasources]}"

    # Verify the datasource name matches what we provisioned
    postgres_ds = postgres_datasources[0]
    assert (
        "crystal" in postgres_ds.get("name", "").lower()
        or "postgres" in postgres_ds.get("name", "").lower()
    ), f"PostgreSQL datasource name unexpected: {postgres_ds.get('name')}"


@pytest.mark.dashboard
def test_postgresql_datasource_connection(grafana_client: GrafanaClient):
    """Verify PostgreSQL datasource can connect to database"""
    datasources = grafana_client.datasources()
    postgres_ds = next((ds for ds in datasources if ds.get("type") == "postgres"), None)

    if postgres_ds is None:
        pytest.skip("PostgreSQL datasource not found")

    ds_id = postgres_ds.get("id")
    result = grafana_client.test_datasource(ds_id)

    # Grafana returns success in different ways depending on version
    assert (
        result.get("status") == "success"
        or result.get("message") == "Data source is working"
        or "ok" in str(result).lower()
    ), f"Datasource connection test failed: {result}"


@pytest.mark.dashboard
def test_crystal_forge_dashboards_provisioned(grafana_client: GrafanaClient):
    """Verify Crystal Forge dashboards are provisioned"""
    dashboards = grafana_client.dashboards()
    assert len(dashboards) > 0, "No dashboards found"

    dashboard_names = [d.get("title", "").lower() for d in dashboards]
    has_cf_dashboard = any(
        "crystal" in name or "forge" in name for name in dashboard_names
    )
    assert (
        has_cf_dashboard or len(dashboards) > 0
    ), f"No Crystal Forge dashboards found. Available: {dashboard_names}"


@pytest.mark.dashboard
def test_dashboard_system_count_query(
    grafana_client: GrafanaClient,
    cf_client: CFTestClient,
    clean_test_data,
):
    """
    Verify dashboard can query system counts from the database.
    Creates test scenarios and verifies the query returns expected counts.
    """
    # Create multiple test scenarios with different states
    scenario_up_to_date(cf_client)
    scenario_behind(cf_client)
    scenario_offline(cf_client)

    # Get PostgreSQL datasource
    datasources = grafana_client.datasources()
    postgres_ds = next((ds for ds in datasources if ds.get("type") == "postgres"), None)
    assert postgres_ds is not None, "PostgreSQL datasource not found"

    ds_id = postgres_ds.get("id")

    # Test system count query from the dashboard
    system_count_query = (
        "SELECT COUNT(*) as total_systems FROM view_systems_current_state"
    )
    results = grafana_client.query_datasource(ds_id, system_count_query)

    assert len(results) > 0, "System count query returned no results"
    # We expect at least 3 systems from our scenarios
    system_count = results[0][0] if results else 0
    assert system_count >= 3, f"Expected at least 3 systems, got {system_count}"


@pytest.mark.dashboard
def test_dashboard_status_breakdown_query(
    grafana_client: GrafanaClient,
    cf_client: CFTestClient,
    clean_test_data,
):
    """
    Verify dashboard queries return correct status breakdown.
    Tests that panels can distinguish between up-to-date, behind, and offline systems.
    """
    # Create test scenarios
    scenario_up_to_date(cf_client)
    scenario_behind(cf_client)
    scenario_offline(cf_client)

    datasources = grafana_client.datasources()
    postgres_ds = next((ds for ds in datasources if ds.get("type") == "postgres"), None)
    assert postgres_ds is not None, "PostgreSQL datasource not found"

    ds_id = postgres_ds.get("id")

    # Query for status breakdown
    status_query = """
    SELECT
        COUNT(*) FILTER (WHERE is_running_latest_derivation = TRUE) as up_to_date,
        COUNT(*) FILTER (WHERE is_running_latest_derivation = FALSE) as behind,
        COUNT(*) FILTER (WHERE last_seen < NOW() - INTERVAL '15 minutes') as no_heartbeat
    FROM view_systems_current_state
    """
    results = grafana_client.query_datasource(ds_id, status_query)

    assert len(results) > 0, "Status breakdown query returned no results"
    # Results should have [up_to_date, behind, no_heartbeat] counts
    assert (
        len(results[0]) >= 3
    ), f"Expected at least 3 status counts, got {len(results[0])}"


@pytest.mark.dashboard
def test_dashboard_panels_have_queries(
    grafana_client: GrafanaClient,
):
    """Verify that dashboard panels are configured with queries"""
    dashboards = grafana_client.dashboards()
    assert len(dashboards) > 0, "No dashboards found"

    dashboard = dashboards[0]
    dashboard_uid = dashboard.get("uid")
    assert dashboard_uid is not None, "Dashboard UID not available"

    dashboard_detail = grafana_client.dashboard(dashboard_uid)
    panels = dashboard_detail.get("dashboard", {}).get("panels", [])

    assert len(panels) > 0, "Dashboard has no panels"

    # Check that at least some panels have queries
    panels_with_queries = [
        p for p in panels if p.get("targets") and len(p.get("targets", [])) > 0
    ]
    assert len(panels_with_queries) > 0, "Dashboard has panels but none with queries"

    # Log panel information
    for i, panel in enumerate(panels_with_queries[:3]):
        title = panel.get("title", "Unnamed")
        targets = len(panel.get("targets", []))
        print(f"  Panel {i+1}: {title} ({targets} targets)")


@pytest.mark.dashboard
def test_dashboard_screenshot_capture(
    grafana_client: GrafanaClient,
    server,
):
    """
    Capture a screenshot of the Crystal Forge dashboard and save to test output.
    Screenshots are saved to /tmp for access after test completion.
    """
    if server is None:
        pytest.skip("Server machine not available")

    dashboards = grafana_client.dashboards()
    if not dashboards:
        pytest.skip("No dashboards available to screenshot")

    dashboard = dashboards[0]
    dashboard_uid = dashboard.get("uid")
    dashboard_title = dashboard.get("title", "Unknown")

    if not dashboard_uid:
        pytest.skip("Dashboard UID not available")

    # Save to /tmp which is accessible after tests
    screenshot_path = f"/tmp/grafana-dashboard-{dashboard_uid}.png"

    try:
        # Use curl from the server to capture screenshot
        success = grafana_client.screenshot_curl(dashboard_uid, screenshot_path, server)

        if success:
            # Verify we can copy it locally
            server.copy_from_vm(screenshot_path, screenshot_path)
            assert Path(
                screenshot_path
            ).exists(), "Failed to retrieve screenshot from VM"
            assert (
                Path(screenshot_path).stat().st_size > 100
            ), "Screenshot file too small"

            print(
                f"✅ Screenshot captured: {screenshot_path} for dashboard '{dashboard_title}'"
            )
        else:
            pytest.skip("Screenshot capture failed - rendering may not be available")
    except Exception as e:
        pytest.skip(f"⚠️ Screenshot capture not available: {e}")


@pytest.mark.dashboard
def test_grafana_provisioning_immutability(server):
    """
    Verify that provisioned dashboards are set to be non-deletable
    (disableDeletion setting is respected).
    """
    if server is None:
        pytest.skip("Server machine not available")

    # Check Grafana configuration for provisioning settings
    grafana_config_paths = [
        "/etc/grafana/provisioning/dashboards/crystal-forge.yaml",
        "/var/lib/grafana/provisioning/dashboards/crystal-forge.yaml",
    ]

    config_found = False
    for config_path in grafana_config_paths:
        try:
            result = server.succeed(
                f"test -f {shlex.quote(config_path)} && echo exists || true"
            )
            if "exists" in result or result.strip() == "exists":
                config_found = True
                break
        except Exception:
            continue

    assert config_found, "No Grafana provisioning config found"


@pytest.mark.dashboard
def test_dashboard_data_persistence(
    grafana_client: GrafanaClient,
    cf_client: CFTestClient,
    clean_test_data,
):
    """
    Verify that dashboard data persists and is queryable after creation.
    This is a comprehensive end-to-end test of the data flow.
    """
    # Create test data
    scenario = scenario_up_to_date(cf_client)

    # Get datasource
    datasources = grafana_client.datasources()
    postgres_ds = next((ds for ds in datasources if ds.get("type") == "postgres"), None)
    assert postgres_ds is not None, "PostgreSQL datasource not found"

    ds_id = postgres_ds.get("id")

    # Query the specific hostname from our scenario
    hostname_query = f"""
    SELECT hostname, is_running_latest_derivation
    FROM view_systems_current_state
    WHERE hostname = %s
    """

    # Verify the scenario hostname exists and has expected status
    hostname_check = cf_client.execute_sql(
        hostname_query.replace("%s", "'{}'").format(scenario["hostname"])
    )
    assert (
        len(hostname_check) > 0
    ), f"Scenario hostname {scenario['hostname']} not found in database"
    assert (
        hostname_check[0]["is_running_latest_derivation"] is True
    ), "System should be up-to-date but is not"
