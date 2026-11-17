"""Focused contracts for the static design export importer."""

import base64
import gzip
import importlib.util
import json
from pathlib import Path
import unittest


spec = importlib.util.spec_from_file_location(
    "import_design_export", Path(__file__).with_name("import-design-export.py")
)
importer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(importer)
A = "00000000-0000-0000-0000-000000000001"
B = "00000000-0000-0000-0000-000000000002"


def html(scripts):
    return ('<!DOCTYPE html>\n<html data-theme="dark"><head><title>Design</title>'
            '<style>body { color: red; }</style></head><body>'
            + scripts + '</body></html>\n')


def resource(code, compressed=True):
    raw = code.encode()
    if compressed:
        raw = gzip.compress(raw, mtime=0)
    return dict(mime="text/javascript", compressed=compressed,
                data=base64.b64encode(raw).decode())


def bundle(template, manifest, **sections):
    sections = {"manifest": manifest, "template": template, **sections}
    return html(''.join(f'<script type="__bundler/{key}">'
                        + json.dumps(value).replace('</', '<\\/') + '</script>'
                        for key, value in sections.items()))


class NormalizeTests(unittest.TestCase):
    def test_order_attributes_and_template_preserved(self):
        before = ('<script src="https://cdn.example/react.js" crossorigin="anonymous">'
                  '</script><script type="text/babel" data-presets="react" '
                  f'src="{B}" data-plugins="transform-modules-umd"></script>'
                  f'<script src="{A}"></script><script>selectViewAndTheme();</script>')
        template = html(before)
        manifest = {A: resource('first();'), B: resource('second();', False)}
        result = importer.normalize(bundle(template, manifest))
        self.assertEqual(result, importer.normalize(bundle(template, dict(reversed(list(manifest.items()))))))
        self.assertLess(result.index('second();'), result.index('first();'))
        self.assertIn('type="text/babel" data-presets="react" data-plugins="transform-modules-umd"', result)
        self.assertIn('<!-- resource-001 -->', result)
        self.assertIn('<script>selectViewAndTheme();</script>', result)
        self.assertEqual(template.split('<body>')[0], result.split('<body>')[0])
        self.assertIn('src="https://cdn.example/react.js" crossorigin="anonymous"', result)
        self.assertEqual(result, importer.normalize(result))

    def test_uuid_churn_does_not_change_output(self):
        one = importer.normalize(bundle(html(f'<script src="{A}"></script>'), {A: resource('ok();')}))
        two = importer.normalize(bundle(html(f'<script src="{B}"></script>'), {B: resource('ok();')}))
        self.assertEqual(one, two)

    def test_escape_closing_script_and_normalize_lf(self):
        result = importer.normalize(bundle(html(f'<script src="{A}"></script>'), {
            A: resource('const s = "</ScRiPt>";\r\nnext();')
        }))
        self.assertIn('"<\\/ScRiPt>";\nnext();', result)
        self.assertEqual(result.count('</script>'), 1)
        self.assertNotIn('\r', result)

    def test_plain_html(self):
        source = html('<script>const x = 1;</script>')
        self.assertEqual(importer.normalize(source.replace('\n', '\r\n')), source)

    def test_missing_reference(self):
        with self.assertRaisesRegex(ValueError, 'Missing resource'):
            importer.normalize(bundle(html(f'<script src="{A}"></script>'), {}))

    def test_unsupported_or_malformed_shapes(self):
        cases = [
            bundle(html(''), {}, ext_resources=["external"]),
            bundle(html(''), {}, page_order=[A]),
            bundle(html('<iframe src="about:blank"></iframe>'), {}),
            bundle(html(''), {A: resource('unused();')}),
            bundle(html(''), {A: {**resource('x'), 'mime': 'text/css'}}),
            bundle(html(''), {A: {**resource('x'), 'data': 'not base64!'}}),
            bundle(html(''), {A: {**resource('x'), 'data': 'eA=='}}),
            bundle(html(''), []),
            bundle({}, {}),
            html('<script src="local.js"></script>'),
            html('<script type="__bundler/manifest">{}</script>'),
            html('<script type="__bundler/manifest">{"x":1,"x":2}</script>'),
            bundle(html(f'<script defer src="{A}"></script>'), {A: resource('x')}),
            'not html',
        ]
        for source in cases:
            with self.subTest(source=source), self.assertRaises(ValueError):
                importer.normalize(source)


if __name__ == '__main__':
    unittest.main()
