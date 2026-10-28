"""Test reference generation, including the actual Nix-generated schema."""

import json
import os
from pathlib import Path
import tempfile
import unittest

from options import generate


class GeneratorTests(unittest.TestCase):
    def test_missing_export_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "source.json").write_text("{}")
            (root / "manifest.json").write_text(json.dumps({"modules": [
                {"name": "missing", "declaration": "modules/nixos/missing/default.nix"}
            ]}))
            with self.assertRaisesRegex(ValueError, "missing"):
                generate(root / "source.json", root / "out", root / "manifest.json", "https://example.test/-/blob/dev")

    def test_dotted_groups_do_not_overwrite(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            declaration = "modules/nixos/example/default.nix"
            options = {name: {"declarations": [declaration], "description": "Description", "type": "boolean"}
                       for name in ["crystal-forge.stig.audit.enable", "crystal-forge.stig.ssh.enable"]}
            (root / "source.json").write_text(json.dumps(options))
            (root / "manifest.json").write_text(json.dumps({"modules": [{"name": "example", "declaration": declaration}]}))
            generate(root / "source.json", root / "out", root / "manifest.json", "https://example.test/-/blob/dev")
            for control in ["audit", "ssh"]:
                content = (root / f"out/options/crystal-forge/stig/{control}.md").read_text()
                self.assertIn(f"crystal-forge.stig.{control}.enable", content)
                self.assertIn("https://example.test/-/blob/dev/" + declaration, content)


@unittest.skipUnless(os.environ.get("DOCS_OPTIONS_JSON"), "requires Nix-generated reference")
class GeneratedReferenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.options = json.loads(Path(os.environ["DOCS_OPTIONS_JSON"]).read_text())
        cls.bundle = Path(os.environ["DOCS_OPTIONS_BUNDLE"])
        cls.coverage = json.loads((cls.bundle / "coverage.json").read_text())

    def test_all_exports_and_counts(self):
        declarations = {p for o in self.options.values() for p in o["declarations"]}
        # These lower bounds catch accidentally evaluating only the main service.
        self.assertGreaterEqual(self.coverage["module_count"], 31)
        self.assertGreaterEqual(self.coverage["option_count"], 200)
        self.assertEqual(len(self.options), self.coverage["option_count"])
        self.assertEqual(sum(self.coverage["groups"].values()), len(self.options))
        for module in self.coverage["modules"]:
            self.assertIn(module["declaration"], declarations)

    def test_namespace_counts_match_export_inventory(self):
        names = [m["name"] for m in self.coverage["modules"]]
        controls = {m["declaration"] for m in self.coverage["modules"]
                    if m["name"].startswith("stig-modules/")}
        presets = {name.removeprefix("stig/") for name in names if name.startswith("stig/")}
        self.assertTrue(controls)
        self.assertTrue(presets)
        tracking = {declaration for name, option in self.options.items()
                    if name.startswith("crystal-forge.stig.active.")
                    for declaration in option["declarations"]}
        self.assertEqual(tracking, controls)
        self.assertGreater(self.coverage["groups"]["services.crystal-forge.attic-metrics"], 0)
        self.assertGreaterEqual(self.coverage["groups"]["services.crystal-forge.server"], 50)
        documented_presets = {name.removeprefix("crystal-forge.stig-presets.").removesuffix(".enable")
                              for name in self.options
                              if name.startswith("crystal-forge.stig-presets.") and name.endswith(".enable")}
        self.assertEqual(documented_presets, presets)

    def test_representative_real_options_are_rendered(self):
        for name in ["services.crystal-forge.server.enable",
                     "services.crystal-forge.attic-metrics.port",
                     "crystal-forge.stig.audit.enable",
                     "crystal-forge.stig.ssh.justification",
                     "crystal-forge.stig-presets.high.enable",
                     "crystal-forge.stig-presets.off.enable",
                     "services.syslog-ng.remote_hosts"]:
            self.assertIn(name, self.options)
            group = "/".join(name.split(".")[:3])
            text = (self.bundle / "options" / (group + ".md")).read_text()
            self.assertIn(f"## `{name}`", text)
            self.assertIn("/-/blob/dev/modules/nixos/", text)

    def test_lazy_values_and_store_paths_not_published(self):
        for option in self.options.values():
            self.assertNotIn("default", option)
            self.assertNotIn("example", option)
            self.assertTrue(option["declarations"])
        for path in (self.bundle / "options").rglob("*.md"):
            self.assertNotIn("/nix/store/", path.read_text())


if __name__ == "__main__":
    unittest.main()
