# Authored facts and the design adapter shared by native integration checks.
#
# Scenario expectations come from two independent sources only:
#
# 1. Facts authored in the scenario file (credentials, names, counts, and
#    other deliberately chosen inputs).
# 2. Records selected by stable ID from the approved design fixture
#    `docs/design/CrystalForge/fixtures/crystal-forge.fixtures.json`.
#
# INVARIANT: An expected value MUST NOT be read from the Crystal Forge
# response or function that the scenario tests. Runtime-generated IDs and
# store paths are captured separately as receipts (see driver.py) and are
# asserted only by relationship to these facts.
#
# The design fixture is presentation data. It does not define backend
# identity, authorization, lifecycle, or evidence rules. Use it for
# comparable presentation states and display identities only.
#
# docs/design/CrystalForge/ is read-only for the integration overhaul. This
# file reads it; it never copies the full fixture set.
{
  pkgs,
  lib,
  inputs,
}: let
  designDir = "${inputs.self}/docs/design/CrystalForge";
  fixturePath = "${designDir}/fixtures/crystal-forge.fixtures.json";
  fixtures = builtins.fromJSON (builtins.readFile fixturePath);

  # Identifies the exact approved reference used by a run. Recorded in every
  # scenario's meta.json so that visual evidence names its reference revision.
  designReference = {
    path = "docs/design/CrystalForge";
    component = "crystal-forge.html";
    sourceRevision = inputs.self.rev or inputs.self.dirtyRev or "unknown";
    fixtureSha256 = builtins.hashFile "sha256" fixturePath;
    fixtureSeed = fixtures._meta.rngSeed or null;
  };

  # Returns the single record whose `id` equals `id` in the collection at the
  # dotted `collection` path, for example `byId "admin.users" "u1"`.
  #
  # Evaluation fails when the collection is missing or when the ID does not
  # match exactly one record. Keys never depend on array position.
  byId = collection: id: let
    records = lib.attrByPath (lib.splitString "." collection) null fixtures;
    matches = builtins.filter (r: (r.id or null) == id) (
      if builtins.isList records
      then records
      else throw "cf-it facts: design fixture collection '${collection}' is not a list"
    );
  in
    if builtins.length matches == 1
    then builtins.head matches
    else throw "cf-it facts: design fixture ${collection} id '${id}' matched ${toString (builtins.length matches)} records";

  # Maps a design fixture user to a local-account fact.
  #
  # The design user supplies display identity (name, email, role label). The
  # caller supplies the local username and password because the design
  # fixture models OIDC users and defines no local credentials.
  localUserFromDesign = {
    id,
    username,
    password,
  }: let
    u = byId "admin.users" id;
    parts = lib.splitString " " u.name;
  in {
    designId = id;
    inherit username password;
    inherit (u) email name role;
    firstName = builtins.head parts;
    lastName = lib.concatStringsSep " " (builtins.tail parts);
  };

  # Offline copy of the approved design example with its four CDN scripts
  # vendored. COMPATIBILITY: these pins MUST stay identical to
  # packages/design-targets/default.nix so both consumers render the same
  # reference bytes and share store paths.
  vendor = {
    react = pkgs.fetchurl {
      url = "https://unpkg.com/react@18.3.1/umd/react.development.js";
      sha256 = "0zsfq9pj3pbpiz9p6k6qflwd33s24kwflbdjxqn8pvdhdkpqyd18";
    };
    reactDom = pkgs.fetchurl {
      url = "https://unpkg.com/react-dom@18.3.1/umd/react-dom.development.js";
      sha256 = "1r09hyz12n03w6fvcnv93ri0mv16wljgkpq4laqqpnrrkig4l17r";
    };
    babel = pkgs.fetchurl {
      url = "https://unpkg.com/@babel/standalone@7.29.0/babel.min.js";
      sha256 = "186f1mfjlcs49p0j0hss1m9cxpbpw9a12imli7kmr48953iaj8r6";
    };
    jszip = pkgs.fetchurl {
      url = "https://unpkg.com/jszip@3.10.1/dist/jszip.min.js";
      hash = "sha256-rMfkFFWoB2W1/Zx+4bgHim0WC7vKRVrq6FTeZclH1Z4=";
    };
  };

  designSite = pkgs.runCommand "cf-it-design-reference" {} ''
    mkdir -p $out/vendor
    cp -r ${designDir}/. $out/
    chmod -R u+w $out
    cp ${vendor.react} $out/vendor/react.development.js
    cp ${vendor.reactDom} $out/vendor/react-dom.development.js
    cp ${vendor.babel} $out/vendor/babel.min.js
    cp ${vendor.jszip} $out/vendor/jszip.min.js
    ${pkgs.gnused}/bin/sed -i -E \
      -e 's#src="https://unpkg.com/react@[^"]*"#src="vendor/react.development.js"#' \
      -e 's#src="https://unpkg.com/react-dom@[^"]*"#src="vendor/react-dom.development.js"#' \
      -e 's#src="https://unpkg.com/@babel/standalone@[^"]*"#src="vendor/babel.min.js"#' \
      -e 's#src="https://unpkg.com/jszip@[^"]*"#src="vendor/jszip.min.js"#' \
      -e 's# integrity="[^"]*"##g' \
      -e 's# crossorigin="anonymous"##g' \
      $out/crystal-forge.html
    if grep -q 'https://unpkg.com' $out/crystal-forge.html; then
      echo "cf-it: design reference still references a CDN script" >&2
      exit 1
    fi
  '';
in {
  inherit designReference byId localUserFromDesign designSite;
}
