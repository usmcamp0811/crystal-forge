# Import with the scenario's root-overlay pkgs. This returns upstream inputs,
# not scanner results. Serve feedRoot as the HTTPS document root for
# nvd.nist.gov, with a test-owned CA trusted by the builder's real requests
# client. Do not disable TLS validation. No .meta files or NVD API are used by
# the pinned vulnix 1.12.4: it GETs gzip-compressed NVD 2.0 JSON directly.
#
# A fresh scanner requests [current year - 5, current year]. The default years
# support VM dates 2026..2031; pin the scenario clock in that interval so the
# 2026 advisory is included. The modified feed contains the same advisory for
# the scanner's incremental update path. Empty year feeds remain real gzip.
#
# Interface: feedRoot is a derivation containing feeds/json/cve/2.0/*;
# facts is independently authored expected data; packages.{vulnerable,corrected}
# are real package derivations for inclusion in the scenario's NixOS closures.
# These packages are inputs to real builds, never pre-completed CF records.
# faultFeedRoot has the same URL layout but invalid upstream JSON. The owner
# can route to it for a real scanner-fault case, with a fresh scanner cache;
# a warm cache skips updates for two hours. Restore feedRoot after the fault.
# This fault changes the upstream input only, never the vulnix executable.
# vulnix exits 1 for this JSONDecodeError. cf-builder classifies exit 1 as
# transient and requeues the attempt. terminalFaultFeedRoot separately provides
# valid JSON with an out-of-range score for a deterministic builder rejection.
{ pkgs, years ? pkgs.lib.range 2021 2031 }:
let
  facts = builtins.fromJSON (builtins.readFile ./facts.json);
  advisory = {
    cve = {
      id = facts.cveId;
      sourceIdentifier = "assurance@example.invalid";
      published = "2026-01-01T00:00:00.000";
      lastModified = "2026-01-01T00:00:00.000";
      vulnStatus = "Analyzed";
      descriptions = [{ lang = "en"; value = facts.description; }];
      configurations = [{
        nodes = [{
          operator = "OR";
          negate = false;
          cpeMatch = [{
            vulnerable = true;
            criteria = "cpe:2.3:a:crystal_forge:${facts.package}:${facts.vulnerableVersion}:*:*:*:*:*:*:*";
            matchCriteriaId = "49016000-0000-4000-8000-000000000001";
          }];
        }];
      }];
      metrics.cvssMetricV31 = [{
        source = "assurance@example.invalid";
        type = "Primary";
        cvssData = {
          version = "3.1";
          vectorString = "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H";
          baseScore = facts.cvssV3;
          baseSeverity = "CRITICAL";
        };
        exploitabilityScore = 3.9;
        impactScore = 5.9;
      }];
    };
  };
  feed = vulnerabilities: pkgs.writeText "assurance-nvd-2.0.json" (builtins.toJSON {
    inherit vulnerabilities;
    resultsPerPage = builtins.length vulnerabilities;
    startIndex = 0;
    totalResults = builtins.length vulnerabilities;
    format = "NVD_CVE";
    version = "2.0";
    timestamp = "2026-01-01T00:00:00.000";
  });
  populated = feed [ advisory ];
  # The real parser accepts numeric scores without a range check. The real
  # builder rejects 11 in assemble_result before it can seal observations.
  # Keep this separate from malformed JSON, whose exit 1 is transient.
  terminalAdvisory = pkgs.lib.recursiveUpdate advisory {
    cve.metrics.cvssMetricV31 = [{
      source = "assurance@example.invalid";
      type = "Primary";
      cvssData = {
        version = "3.1";
        vectorString = "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H";
        baseScore = 11;
        baseSeverity = "CRITICAL";
      };
    }];
  };
  terminalFeed = feed [ terminalAdvisory ];
  empty = feed [];
  malformed = pkgs.writeText "assurance-invalid-nvd.json" "{invalid upstream JSON\n";
  package = version: pkgs.runCommand "${facts.package}-${version}" {
    pname = facts.package;
    inherit version;
  } ''
    mkdir -p "$out/share/cf-assurance"
    printf '%s\n' '${version}' > "$out/share/cf-assurance/version"
  '';
in rec {
  inherit facts;
  packages = {
    vulnerable = package facts.vulnerableVersion;
    corrected = package facts.correctedVersion;
  };
  feedRoot = pkgs.runCommand "assurance-nvd-feeds" {
    nativeBuildInputs = [ pkgs.gzip ];
  } ''
    mkdir -p "$out/feeds/json/cve/2.0"
    ${pkgs.lib.concatMapStringsSep "\n" (year: ''
      gzip -n -c ${if year == 2026 then populated else empty} > "$out/feeds/json/cve/2.0/nvdcve-2.0-${toString year}.json.gz"
    '') years}
    gzip -n -c ${populated} > "$out/feeds/json/cve/2.0/nvdcve-2.0-modified.json.gz"
    gzip -n -c ${populated} > "$out/feeds/json/cve/2.0/nvdcve-2.0-recent.json.gz"
  '';
  faultFeedRoot = pkgs.runCommand "assurance-invalid-nvd-feeds" {
    nativeBuildInputs = [ pkgs.gzip ];
  } ''
    mkdir -p "$out/feeds/json/cve/2.0"
    ${pkgs.lib.concatMapStringsSep "\n" (name: ''
      gzip -n -c ${malformed} > "$out/feeds/json/cve/2.0/nvdcve-2.0-${name}.json.gz"
    '') ((map toString years) ++ [ "modified" "recent" ])}
  '';
  terminalFaultFeedRoot = pkgs.runCommand "assurance-invalid-cvss-nvd-feeds" {
    nativeBuildInputs = [ pkgs.gzip ];
  } ''
    mkdir -p "$out/feeds/json/cve/2.0"
    ${pkgs.lib.concatMapStringsSep "\n" (name: ''
      gzip -n -c ${terminalFeed} > "$out/feeds/json/cve/2.0/nvdcve-2.0-${name}.json.gz"
    '') ((map toString years) ++ [ "modified" "recent" ])}
  '';
  # Compose this module into the existing services VM. The coordinator owns
  # certificate provisioning, builder trust, hostname routing and scan opt-in.
  servicesModule = import ./service.nix { inherit feedRoot; };
  feedCommands = let
    select = root: "ln -sfn ${root} /run/cf-assurance-nvd/next && "
      + "mv -Tf /run/cf-assurance-nvd/next /run/cf-assurance-nvd/active";
  in {
    normal = select feedRoot;
    fault = select faultFeedRoot;
    terminalFault = select terminalFaultFeedRoot;
  };
}
