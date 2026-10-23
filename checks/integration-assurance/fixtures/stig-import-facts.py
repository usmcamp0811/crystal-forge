"""Emits ordinary JSON facts from the exact source XML, without CF dependencies.

Coordinator interface: run `python3 stig-import-facts.py FIXTURE_DIRECTORY` in a
Nix build with Python, then merge the decoded object as facts.stigImport. Install
the four XML files at each emitted controllerPath under /etc/cf-it/stig/. The
optional second argument changes that controller directory, not the source.
The two official inputs are byte copies of fixtures/xccdf/; do not reserialize
them. Digests identify input bytes only and do not confer product trust.
"""

import hashlib
import json
from pathlib import Path
import re
import sys
import xml.etree.ElementTree as ET


def source_facts(path: Path, controller_directory: str) -> dict:
    """Returns byte identity plus namespace-qualified benchmark and rule facts."""
    content = path.read_bytes()
    root = ET.fromstring(content)
    namespace = "http://checklists.nist.gov/xccdf/1.1"
    def q(name: str) -> str:
        return "{" + namespace + "}" + name

    assert root.tag == q("Benchmark"), (path, root.tag)
    version = root.findtext(q("version"))
    release_info = root.findtext(q("plain-text") + "[@id='release-info']")
    release = re.search(r"Release:\s*(\d+)", release_info).group(1)
    rules = []
    for group in root.iter(q("Group")):
        for rule in group.findall(q("Rule")):
            rules.append({
                "id": rule.attrib["id"], "stableKey": group.attrib["id"],
                "title": rule.findtext(q("title")),
                "version": rule.findtext(q("version")),
                "severity": rule.attrib["severity"],
            })
    assert len(rules) == len(list(root.iter(q("Rule")))) > 0
    assert len({r["id"] for r in rules}) == len(rules)
    assert len({r["stableKey"] for r in rules}) == len(rules)
    assert all(re.fullmatch(r"V-\d+", r["stableKey"]) for r in rules)
    return {
        "filename": path.name, "controllerPath": controller_directory + "/" + path.name,
        "sha256": hashlib.sha256(content).hexdigest(), "byteLength": len(content),
        "namespace": namespace, "benchmarkId": root.attrib["id"],
        "title": root.findtext(q("title")), "benchmarkVersion": version,
        "releaseInfo": release_info, "release": "V" + version + "R" + release,
        "ruleCount": len(rules), "profileCount": len(list(root.iter(q("Profile")))),
        "rules": sorted(rules, key=lambda r: r["id"]),
    }


def build_facts(directory: Path, controller_directory: str = "/etc/cf-it/stig") -> dict:
    """Derives both releases and a stable, discriminating cross-release subset."""
    releases = {release: source_facts(directory / (
        "stig-import-U_Anduril_NixOS_STIG_" + release + "_Manual-xccdf.xml"),
        controller_directory) for release in ("V1R1", "V1R2")}
    for release, facts in releases.items():
        assert facts["release"] == release
    first, second = (releases[r] for r in ("V1R1", "V1R2"))
    assert first["sha256"] != second["sha256"]
    by_key = {r["stableKey"]: r for r in second["rules"]}
    changed = [r["stableKey"] for r in first["rules"]
               if r["stableKey"] in by_key and r != by_key[r["stableKey"]]]
    assert len(changed) >= 3, "Need distinguishable stable identities across releases"
    invalid = {}
    for name in ("truncated", "non-xccdf"):
        path = directory / ("stig-import-" + name + ".xml")
        content = path.read_bytes()
        if name == "truncated":
            try:
                ET.fromstring(content)
            except ET.ParseError:
                pass
            else:
                raise AssertionError("Truncated input must be malformed XML")
        else:
            assert ET.fromstring(content).tag == "{urn:assurance:inventory}inventory"
        invalid[name] = {"filename": path.name,
                         "controllerPath": controller_directory + "/" + path.name,
                         "sha256": hashlib.sha256(content).hexdigest(), "byteLength": len(content)}
    return {"releases": releases, "invalid": invalid, "stableSubset": sorted(changed)[:3],
            "mappingRationale": "Reviewed earlier-release policy as partial support for the same stable control in V1R2; no execution result is claimed."}


if __name__ == "__main__":
    print(json.dumps(build_facts(Path(sys.argv[1]), *sys.argv[2:]), sort_keys=True))
