#!/usr/bin/env python3
"""Normalize the single-page design export without executing its payload.

Only embedded JavaScript resources are supported. CDN references stay intact;
other resource kinds and nested pages fail rather than losing export behavior.
Template text is preserved apart from script inlining and UTF-8/LF normalization.
"""

import argparse
import base64
import gzip
from html.parser import HTMLParser
import json
from pathlib import Path
import re
import zlib


UUID = re.compile(r"[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}\Z")
ATTRIBUTE = re.compile(
    r'''\s+(?P<name>[^\s=/>]+)(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+))?'''
)


class Document(HTMLParser):
    """Locate script spans without serializing or reformatting the template."""

    def __init__(self, text):
        super().__init__(convert_charrefs=False)
        self.text = text
        self.lines = [0] + [m.end() for m in re.finditer("\n", text)]
        self.scripts = []
        self.current = None
        self.tags = set()
        self.feed(text)
        self.close()
        if self.current:
            raise ValueError("Unclosed script")

    def position(self):
        line, column = self.getpos()
        return self.lines[line - 1] + column

    def handle_starttag(self, tag, attrs):
        self.tags.add(tag)
        if tag in {"iframe", "frame", "frameset", "object", "embed"}:
            raise ValueError("Nested pages/embedded documents are unsupported")
        if tag == "script":
            if len(dict(attrs)) != len(attrs):
                raise ValueError("Duplicate script attributes")
            start = self.position()
            opening = self.get_starttag_text()
            self.current = (start, start + len(opening), opening, dict(attrs))

    def handle_startendtag(self, tag, attrs):
        if tag == "script":
            raise ValueError("Self-closing script is unsupported")
        self.handle_starttag(tag, attrs)

    def handle_endtag(self, tag):
        if tag == "script" and self.current:
            start, body, opening, attrs = self.current
            end = self.position()
            close = self.text.index(">", end) + 1
            self.scripts.append((start, close, opening, attrs, self.text[body:end]))
            self.current = None


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"Duplicate JSON key: {key}")
        result[key] = value
    return result


def lf(text):
    return text.replace("\r\n", "\n").replace("\r", "\n")


def normalize(source):
    """Return deterministic HTML, or raise ValueError for unsupported input.

    Resource labels follow template order, never manifest order or bundle UUIDs.
    Already standalone HTML is accepted unchanged except for LF normalization.
    """
    source = lf(source)
    document = Document(source)
    bundle = {}
    for _, _, _, attrs, body in document.scripts:
        kind = attrs.get("type") or ""
        if kind.startswith("__bundler/"):
            if kind in bundle:
                raise ValueError(f"Duplicate bundle section: {kind}")
            bundle[kind] = json.loads(body, object_pairs_hook=unique_object)
    resources = {}
    template = source
    if bundle:
        required = {"__bundler/manifest", "__bundler/template"}
        allowed = required | {"__bundler/ext_resources", "__bundler/page_order"}
        if not required <= bundle.keys() or bundle.keys() - allowed:
            raise ValueError("Missing or unsupported bundle sections")
        for kind in allowed - required:
            if bundle.get(kind, []) != []:
                raise ValueError(f"Unsupported external resources/nested pages: {kind}")
        resources = bundle["__bundler/manifest"]
        template = bundle["__bundler/template"]
        if not isinstance(resources, dict) or not isinstance(template, str):
            raise ValueError("Expected an object manifest and string template")
        template = lf(template)
    document = Document(template)
    if not {"html", "head", "body"} <= document.tags or not re.match(
        r"\s*<!doctype html\s*>", template, re.I
    ):
        raise ValueError("Expected a complete HTML document")
    decoded = {}
    for key, resource in resources.items():
        if not UUID.fullmatch(key):
            raise ValueError(f"Unsupported resource ID: {key}")
        if (not isinstance(resource, dict)
                or set(resource) != {"mime", "compressed", "data"}
                or resource["mime"] != "text/javascript"
                or type(resource["compressed"]) is not bool
                or not isinstance(resource["data"], str)):
            raise ValueError(f"Unsupported resource shape: {key}")
        try:
            raw = base64.b64decode(resource["data"], validate=True)
            if resource["compressed"]:
                raw = gzip.decompress(raw)
            decoded[key] = lf(raw.decode("utf-8"))
        except (ValueError, OSError, EOFError, zlib.error) as error:
            raise ValueError(f"Invalid resource {key}: {error}") from error
    used = set()
    pieces = []
    cursor = 0
    for start, end, opening, attrs, body in document.scripts:
        src = attrs.get("src")
        if (attrs.get("type") or "").startswith("__bundler/"):
            raise ValueError("Nested bundle template is unsupported")
        if src is None:
            if "src" in attrs:
                raise ValueError("Script src requires a value")
            continue
        if src.startswith(("https://", "http://", "//")):
            continue
        if src not in decoded:
            raise ValueError(f"Missing resource or unsupported script src: {src}")
        if body.strip():
            raise ValueError(f"Resource script also has inline content: {src}")
        # External-only semantics cannot be retained when the script is inline.
        if {"async", "defer", "integrity"} & attrs.keys():
            raise ValueError(f"Unsupported external-only script attributes: {src}")
        used.add(src)
        label = f"resource-{len(pieces) + 1:03d}"
        code = re.sub(r"</script", lambda match: "<\\/" + match[0][2:],
                      decoded[src], flags=re.I)
        opening = ATTRIBUTE.sub(
            lambda match: "" if match["name"].lower() == "src" else match[0],
            opening,
        )
        pieces.append(template[cursor:start] + f"<!-- {label} -->\n"
                      + opening + "\n" + code
                      + "\n</script>")
        cursor = end
    if decoded.keys() - used:
        raise ValueError("Unused resources: " + ", ".join(sorted(decoded.keys() - used)))
    return "".join(pieces) + template[cursor:]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    try:
        result = normalize(args.source.read_text(encoding="utf-8"))
        args.output.write_bytes(result.encode("utf-8"))
    except (ValueError, OSError) as error:
        parser.exit(1, f"Import failed: {error}\n")


if __name__ == "__main__":
    main()
