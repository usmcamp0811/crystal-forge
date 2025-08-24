import pytest


@pytest.mark.vm_only
def test_server_ports_listen(server, cf_ports, wait_listening):
    wait_listening(server, cf_ports["db_vm"])
    wait_listening(server, cf_ports["api_vm"])
