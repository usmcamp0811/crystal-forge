# Builder and server runtime packaging

```sh
nix eval --raw .#checks.x86_64-linux.builder-evaluator-packaging.drvPath
nix build --no-link -L .#checks.x86_64-linux.builder-evaluator-packaging
```

Use `path:.#` while required new source files remain untracked in a shared task
worktree. This check needs the builder and core server packages; it does not
build the browser UI or start host services.

The check asserts that `pkgs.nix-eval-jobs.nix` is first in both service PATH
lists and that the evaluator-bound Niks3 package is present. It checks standalone
component wrappers and the public builder wrapper. Runtime probes require
`command -v nix` to resolve to the exact evaluator Nix and require
`command -v niks3` and `niks3 --help` to succeed. The Niks3 wrapper must bind the
same evaluator Nix. Finally, the packaged Nix version must match the version
reported by `nix-eval-jobs` itself.

The Niks3 override changes only its Nix dependency. The pinned upstream Niks3
version remains 1.6.0.

## CI

The `.gitlab-ci.yml` `flake-check` matrix includes
`CHECK_NAME: builder-evaluator-packaging`. The generated job
`flake-check: [builder-evaluator-packaging]` is blocking on merge requests and
`main`, uses the existing `nix` runner tag, and builds
`.#checks.x86_64-linux.builder-evaluator-packaging`. Matrix membership does not
establish a pass; review the job result for the exact commit under review.
