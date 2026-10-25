{pkgs, ...}: {
  # This receiving store starts empty on the existing target VM. Its reader
  # owns distinct transport credentials and is untrusted by the local daemon.
  # Trust consists only of the configured CF publisher public keys, never the
  # separate receiver-enforcement fixture's signing key or host cache roots.
  users.groups.cf-cold-reader = {};
  users.users.cf-cold-reader = {
    isSystemUser = true;
    group = "cf-cold-reader";
    home = "/var/lib/cf-cold-reader";
    createHome = true;
    shell = pkgs.bashInteractive;
  };
  systemd.services.cf-it-cold-acquisition = {
    description = "Signature-enforcing cold closure acquisition on target";
    wantedBy = [];
    environment = {
      NIX_DAEMON_SOCKET_PATH = "/run/cf-it-cold-acquisition/socket";
      NIX_USER_CONF_FILES = "/dev/null";
      NIX_CONFIG = "";
    };
    serviceConfig = {
      RuntimeDirectory = "cf-it-cold-acquisition";
      RuntimeDirectoryMode = "0755";
      StateDirectory = "cf-it-cold-acquisition";
      StateDirectoryMode = "0755";
    };
    script = ''
      exec ${pkgs.crystal-forge.default.evaluatorNix}/bin/nix-daemon \
        --store 'local?root=/var/lib/cf-it-cold-acquisition&require-sigs=true' \
        --option trusted-users root --option allowed-users cf-cold-reader \
        --option trusted-public-keys "$(cat /run/cf-it-publisher-keys.pub)" \
        --option extra-trusted-public-keys "" --option secret-key-files "" \
        --option substituters "" --option builders "" --option fallback false
    '';
  };
}
