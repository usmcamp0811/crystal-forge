{pkgs, config, ...}: {
  environment.systemPackages = [pkgs.iptables pkgs.iproute2];
  virtualisation.vlans = [1 2];
  networking.interfaces.eth2.ipv6.addresses = [{
    address = "2001:db8:2::" + {
      builderA = "1";
      builderB = "2";
      controller = "3";
      services = "4";
      targetA = "5";
    }.${config.networking.hostName};
    prefixLength = 64;
  }];
  users.groups.net-control = {};
  users.users.net-control = {isSystemUser = true; group = "net-control"; uid = 4500;};
  # eth1 is the product VLAN. eth2 is a controlled outside network; eth0 is
  # QEMU NAT. Only the separate control UID can reach the outside HTTP probe.
  # Product actors retain isolation throughout the positive and negative tests.
  networking.firewall.extraCommands = ''
    iptables -A OUTPUT -o eth0 -p udp --dport 67 -d 255.255.255.255/32 -j ACCEPT
    iptables -A OUTPUT -o eth0 -p udp --dport 67 -d 10.0.2.2/32 -j ACCEPT
    iptables -A OUTPUT -o eth0 -j REJECT
    ip6tables -A OUTPUT -o eth0 -p ipv6-icmp -d fe80::/10 -j ACCEPT
    ip6tables -A OUTPUT -o eth0 -p ipv6-icmp -d ff02::/16 -j ACCEPT
    ip6tables -A OUTPUT -o eth0 -j REJECT
    iptables -A OUTPUT -o eth2 -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT
    ip6tables -A OUTPUT -o eth2 -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT
    ip6tables -A OUTPUT -o eth2 -p ipv6-icmp -d fe80::/10 -j ACCEPT
    ip6tables -A OUTPUT -o eth2 -p ipv6-icmp -d ff02::/16 -j ACCEPT
    # Neighbor discovery replies can target the peer's global VLAN address.
    # Permit only neighbor discovery, not arbitrary outside ICMP or TCP.
    ip6tables -A OUTPUT -o eth2 -p ipv6-icmp --icmpv6-type neighbour-solicitation -d 2001:db8:2::/64 -j ACCEPT
    ip6tables -A OUTPUT -o eth2 -p ipv6-icmp --icmpv6-type neighbour-advertisement -d 2001:db8:2::/64 -j ACCEPT
    iptables -A OUTPUT -o eth2 -d 192.168.2.4 -p tcp --dport 9080 -m owner --uid-owner 4500 -j ACCEPT
    ip6tables -A OUTPUT -o eth2 -d 2001:db8:2::4 -p tcp --dport 9080 -m owner --uid-owner 4500 -j ACCEPT
    iptables -A OUTPUT -o eth2 -d 192.168.2.4 -p tcp --dport 9080 -m comment --comment cf-it-probe-v4 -j REJECT --reject-with tcp-reset
    ip6tables -A OUTPUT -o eth2 -d 2001:db8:2::4 -p tcp --dport 9080 -m comment --comment cf-it-probe-v6 -j REJECT --reject-with tcp-reset
    iptables -A OUTPUT -o eth2 -j REJECT
    ip6tables -A OUTPUT -o eth2 -j REJECT
  '';
}
