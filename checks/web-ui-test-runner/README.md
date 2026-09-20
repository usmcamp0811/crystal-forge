# Web UI Test Runner Check

This check is a regression test for `web-ui-test`, the host-side script that
runs the Playwright browser harness (`checks/web-ui/tests/integration-test.js`)
against the persistent local development stack instead of inside a NixOS VM.
It starts no services and no virtual machine; `web-ui-test.sh` itself is
tested, but nothing it would normally launch actually runs.

## What it verifies

The contract `web-ui-test` adds around the shared browser harness:

- Workflow selection — the requested step names are passed through
  correctly.
- Rejection of workflows that require the NixOS VM (workflows not tagged as
  host-runnable in `coverage-manifest.json`'s `settings.devStackWorkflows`)
  when run against the host-side dev stack instead of the VM.
- Development-stack readiness reporting.
- Artifact creation.
- Exit-status propagation from the underlying Playwright run back through
  the wrapper script.

## Why it is a separate check

`web-ui-test` exists so a developer can iterate on Playwright workflows
against `run-ui-dev`'s persistent stack without paying the cost of a NixOS
VM boot on every run. That wrapper script has its own selection and
readiness logic that is easy to regress silently (for example, accidentally
letting a VM-only workflow run against the host stack and fail confusingly).
This check protects that logic directly, independent of the VM-based
`web-ui` check.

## Run it

```sh
nix build .#checks.x86_64-linux.web-ui-test-runner --print-build-logs
```

No VM, no live services. `bash`, `coreutils`, `gnugrep`, `gnused`, and
`nodejs` are the only sandbox dependencies.

## Out of scope

- This does not run any real Playwright browser session or serve the actual
  web UI; it tests the wrapper script's own decision logic in isolation.

## CI

Part of the `.gitlab-ci.yml` `flake-check` matrix
(`CHECK_NAME: web-ui-test-runner`), so it runs on every merge request and on
`main`.

## Related files

- `../web-ui/tests/web-ui-test-runner-test.sh` — the test script this check
  runs.
- `../web-ui/tests/web-ui-test.sh` — the wrapper script under test.
- `../web-ui/coverage-manifest.json` — the manifest whose
  `settings.devStackWorkflows` this check's assertions depend on.
