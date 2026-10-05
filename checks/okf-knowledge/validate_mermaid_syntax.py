#!/usr/bin/env python3
"""Checks every repository Mermaid source with the pinned Mermaid parser.

This check parses Mermaid without starting Chromium. The parser module comes
from the same `mermaid-cli` Nix package used by the manual SVG renderer. The
conversion ledger is checked before parsing, so diagram IDs, semantic review
records, destinations, and Mermaid types remain mandatory.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

NODE_CHECK = r'''
const { default: mermaid } = await import(process.env.MERMAID_MODULE);
const chunks = [];
for await (const chunk of process.stdin) chunks.push(chunk);
const diagrams = JSON.parse(Buffer.concat(chunks).toString("utf8"));
const errors = [];
for (const diagram of diagrams) {
  try {
    await mermaid.parse(diagram.source);
  } catch (error) {
    errors.push(`${diagram.path} block ${diagram.index + 1}: ${error.message}`);
  }
}
console.log(JSON.stringify({ parsed: diagrams.length, errors }));
process.exitCode = errors.length === 0 ? 0 : 1;
'''


def collect(root: Path, render_mermaid) -> list[dict[str, str | int]]:
    diagrams: list[dict[str, str | int]] = []
    for path in render_mermaid.sources(root):
        relative = path.relative_to(root).as_posix()
        if path.suffix.lower() == ".mmd":
            bodies = [(path.read_text(encoding="utf-8"), 1)]
        else:
            bodies = list(render_mermaid.blocks(path))
        diagrams.extend(
            {"path": relative, "index": index, "source": body}
            for index, (body, _line) in enumerate(bodies)
        )
    return diagrams


def parse_sources(
    node: str,
    mermaid_module: str,
    diagrams: list[dict[str, str | int]],
) -> tuple[int, dict[str, object], str]:
    """Parses Mermaid sources without launching a browser."""
    env = os.environ.copy()
    env["MERMAID_MODULE"] = mermaid_module
    loader_source = '''
export async function resolve(specifier, context, nextResolve) {
  if (specifier === "dompurify") {
    return {
      url: "data:text/javascript,export default {addHook(){},sanitize(value){return value}}",
      shortCircuit: true,
    };
  }
  return nextResolve(specifier, context);
}
'''
    with tempfile.TemporaryDirectory(prefix="mermaid-syntax-") as temporary:
        loader = Path(temporary) / "dompurify-loader.mjs"
        loader.write_text(loader_source, encoding="utf-8")
        result = subprocess.run(
            [node, "--no-warnings", "--experimental-loader", loader.as_uri(),
             "--input-type=module", "-e", NODE_CHECK],
            input=json.dumps(diagrams),
            text=True,
            capture_output=True,
            env=env,
        )
    try:
        parsed = json.loads(result.stdout)
    except json.JSONDecodeError:
        parsed = {"parsed": 0, "errors": [result.stderr.strip() or result.stdout.strip()]}
    return result.returncode, parsed, result.stderr


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", default=".")
    parser.add_argument("--node", required=True)
    parser.add_argument("--mermaid-module", required=True)
    parser.add_argument("--report", required=True)
    args = parser.parse_args()
    root = Path(args.repo_root).resolve()
    sys.path.insert(0, str(root / "checks/okf-knowledge"))
    import render_mermaid

    errors = render_mermaid.check_ledger(root)
    diagrams = collect(root, render_mermaid)
    status, parsed, diagnostic = parse_sources(args.node, args.mermaid_module, diagrams)
    errors.extend(parsed["errors"])
    report = Path(args.report)
    report.parent.mkdir(parents=True, exist_ok=True)
    report.write_text(json.dumps({"parsed": parsed["parsed"], "errors": errors}, indent=2) + "\n", encoding="utf-8")
    print(f"Mermaid syntax: {parsed['parsed']} block(s) parsed, {len(errors)} error(s); report {report}")
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    return status


if __name__ == "__main__":
    raise SystemExit(main())
