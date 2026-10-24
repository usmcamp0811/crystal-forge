"""Generate the schema-only OKF reference and fail on missing module coverage."""

import argparse
import json
from pathlib import Path
from urllib.parse import quote

import yaml


LIMITATIONS = (
    "Generated from all exported Crystal Forge NixOS modules with the project's "
    "Snowfall library. This is a schema reference, not an evaluated host or a "
    "validated deployment configuration. No systems or databases are built. "
    "Defaults and examples are omitted without evaluating their values because "
    "they can depend on host settings or application packages. Hidden/internal "
    "options are omitted. STIG tracking declarations reflect the modules' "
    "default enabled state; conditional declarations for the disabled state "
    "are not part of this snapshot. Enabling a preset can change runtime policy; "
    "this reference does not assert compliance."
)


def group_name(name):
    """Return a stable group without treating dots as filename extensions."""
    parts = name.split(".")
    return ".".join(parts[:3])


def generate(source, output, manifest, repository_url):
    """Write an OKF bundle after checking every export has a declaration."""
    options = json.loads(Path(source).read_text())
    modules = json.loads(Path(manifest).read_text())["modules"]
    declarations = {p for option in options.values() for p in option["declarations"]}
    missing = [m["name"] for m in modules if m["declaration"] not in declarations]
    if missing:
        raise ValueError("Exported modules without documented options: " + ", ".join(missing))
    if not modules or not options:
        raise ValueError("The exported module reference must not be empty")
    root = Path(output) / "options"
    root.mkdir(parents=True, exist_ok=True)
    groups = {}
    for name, option in sorted(options.items()):
        groups.setdefault(group_name(name), []).append((name, option))
    coverage = {
        "module_count": len(modules),
        "option_count": len(options),
        "groups": {group: len(members) for group, members in groups.items()},
        "modules": modules,
    }
    (Path(output) / "coverage.json").write_text(json.dumps(coverage, indent=2) + "\n")

    def page(path, title, body):
        meta = {
            "title": title,
            "type": "Reference",
            "description": f"Generated NixOS module schema reference: {title}.",
            "tags": ["nixos", "options", "generated"],
            "implementation_status": "implemented",
        }
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("---\n" + yaml.safe_dump(meta, sort_keys=False)
                        + "---\n\n# " + title + "\n\n" + body + "\n")

    index = [LIMITATIONS, f"\n{len(options)} options from {len(modules)} exported modules.\n"]
    for group, members in groups.items():
        relative = "/".join(group.split(".")) + ".md"
        index.append(f"- [{group}]({relative}) ({len(members)} options)")
        lines = [LIMITATIONS, "\n[All module options](" + "../" * (len(group.split('.')) - 1) + "index.md)\n"]
        for name, option in members:
            description = option.get("description") or "No description supplied by the module."
            if isinstance(description, dict):
                description = description["text"]
            lines.extend([f"## `{name}`\n", description, f"\n**Type:** {option.get('type', 'unspecified')}\n"])
            links = [f"[`{p}`]({repository_url.rstrip('/')}/{quote(p, safe='/')})"
                     for p in option["declarations"]]
            lines.append("**Declared in:** " + ", ".join(links) + "\n")
        page(root / relative, group, "\n".join(lines))
    index.append("\n## Exported modules\n")
    for module in modules:
        declaration = module["declaration"]
        index.append(f"- [`{module['name']}`]({repository_url.rstrip('/')}/{quote(declaration, safe='/')})")
    page(root / "index.md", "NixOS module options", "\n".join(index))
    return coverage


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--manifest", required=True, type=Path)
    parser.add_argument("--repository-url", required=True)
    args = parser.parse_args()
    generate(args.source, args.output, args.manifest, args.repository_url)
