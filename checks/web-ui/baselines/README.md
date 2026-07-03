# Web UI approved screenshot baselines

PNG files in this directory are the **approved golden baselines** for the
web-ui check. Each `<step-name>.png` corresponds to a step in
`../coverage-manifest.json`.

- Steps whose manifest `baseline` policy is `advisory` are compared and
  reported (diff images land in the check output under `screenshots/diffs/`)
  but never fail the check.
- Steps whose policy is `strict` must match their baseline within the
  configured threshold — a missing baseline or an excess diff fails the check.

To approve new/changed screenshots, run `../approve-baselines.sh` with a
directory of freshly captured screenshots (from a `nix build` result or a
downloaded CI artifact), review the git diff, and commit.

See `docs/web-ui-check.md` for the full workflow.
