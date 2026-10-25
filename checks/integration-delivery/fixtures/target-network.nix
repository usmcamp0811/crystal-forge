{...}: {
  # Reproduce the existing five-node test VLAN identity in standalone Git
  # candidates. VLAN declarations alone do not assign guest addresses outside
  # runNixOSTest. Keep node numbering aligned with the explicit topology.
  networking.interfaces.eth1 = {
    ipv4.addresses = [{address = "192.168.1.5"; prefixLength = 24;}];
    ipv6.addresses = [{address = "2001:db8:1::5"; prefixLength = 64;}];
  };
  networking.interfaces.eth2.ipv4.addresses = [{address = "192.168.2.5"; prefixLength = 24;}];
  networking.extraHosts = ''
    192.168.1.1 builderA
    192.168.1.2 builderB
    192.168.1.3 controller
    192.168.1.4 services
    192.168.1.5 targetA
  '';
}
