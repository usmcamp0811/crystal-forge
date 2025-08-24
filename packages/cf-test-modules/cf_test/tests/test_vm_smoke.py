import pytest


@pytest.mark.vm_only
def test_daemons_running(machine):
    assert machine is not None
    machine.wait_until_succeeds("systemctl is-active --quiet postgresql")
    machine.wait_until_succeeds("systemctl is-active --quiet crystal-forge-server")
