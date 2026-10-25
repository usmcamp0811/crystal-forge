{pkgs, ...}: {
  # Use a separate database and physical store: normal publication has already
  # populated the services VM's main store. No destination import runs as root.
  systemd.services.cf-it-signature-receiver = {
    description = "Delivery signature-enforcing receiver";
    wantedBy = [];
    environment = {
      NIX_DAEMON_SOCKET_PATH = "/run/cf-it-signature-receiver/socket";
      NIX_USER_CONF_FILES = "/dev/null";
      NIX_CONFIG = "";
    };
    serviceConfig = {
      User = "root";
      RuntimeDirectory = "cf-it-signature-receiver";
      RuntimeDirectoryMode = "0755";
      StateDirectory = "cf-it-signature-receiver";
      StateDirectoryMode = "0755";
    };
    # The cache user is allowed but NOT trusted by this receiver. Its trust
    # policy stays fixed while the test changes only source signature metadata.
    script = ''
      exec ${pkgs.crystal-forge.default.evaluatorNix}/bin/nix-daemon \
        --store 'local?root=/var/lib/cf-it-signature-receiver&require-sigs=true' \
        --option trusted-users root --option allowed-users cache \
        --option trusted-public-keys "$(cat /run/cf-it-receiver-sign.pub)" \
        --option secret-key-files "" --option substituters "" \
        --option builders "" --option fallback false --option max-jobs 0
    '';
  };
}
