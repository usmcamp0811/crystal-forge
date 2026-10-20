# Supplies only the local upstream NVD vhost. Import on the existing services
# VM, never as another test or node. Certificate paths refer to coordinator-
# provisioned files; the certificate must cover nvd.nist.gov.
{ feedRoot }:
{ sslCertificate, sslCertificateKey, listenAddress ? "0.0.0.0", port ? 443 }:
{ ... }: {
  systemd.tmpfiles.rules = [
    "d /run/cf-assurance-nvd 0755 root root -"
    "L /run/cf-assurance-nvd/active - - - - ${feedRoot}"
  ];
  services.nginx = {
    enable = true;
    virtualHosts."nvd.nist.gov" = {
      inherit sslCertificate sslCertificateKey;
      listen = [{ addr = listenAddress; inherit port; ssl = true; }];
      root = "/run/cf-assurance-nvd/active";
      locations."/feeds/json/cve/2.0/".extraConfig = ''
        # Feed switching must affect the next request, including conditional
        # requests from vulnix. Do not retain an old open file or return 304.
        open_file_cache off;
        etag off;
        if_modified_since off;
        gzip off;
        add_header Cache-Control "no-store" always;
        try_files $uri =404;
      '';
      locations."/".return = "404";
    };
  };
  networking.firewall.allowedTCPPorts = [ port ];
}
