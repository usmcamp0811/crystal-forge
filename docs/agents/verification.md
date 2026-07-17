# Verification Guide

Verification must prove the active task's acceptance criteria while remaining proportional to risk. Run project commands through `nix develop`.

The examples below reflect common Crystal Forge targets. Confirm current flake attributes and package manifests before using them; repository definitions are authoritative.

## Baseline matrix

| Change | Expected verification |
| --- | --- |
| Rust formatting | `nix develop -c cargo fmt --manifest-path <manifest> -- --check` |
| Server/agent/builder logic | Targeted tests for the affected package or module, then the relevant Nix package build |
| Dioxus state or component logic | Web UI formatting and targeted tests, then `nix build .#web-ui` |
| User-visible UI | Relevant UI tests plus authoritative `web-ui` integration check and screenshot |
| API contract shared across components | Tests/builds for every affected producer and consumer |
| SQLx checked query | SQLx preparation plus affected server tests/build |
| Migration/schema | Migration against isolated dev DB, SQLx preparation, and affected integration tests |
| Nix package or module | Targeted Nix build/check; full flake check if impact is cross-cutting |
| CI/release behavior | Relevant local equivalent and `nix flake check --keep-going` |

## Common Crystal Forge commands

Use the subset applicable to the change:

```bash
nix develop -c cargo fmt --manifest-path packages/default/Cargo.toml -- --check
nix develop -c cargo test --manifest-path packages/default/Cargo.toml
nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check
nix develop -c cargo test --manifest-path packages/web-ui/Cargo.toml
nix build .#packages.x86_64-linux.server --no-link
nix build .#packages.x86_64-linux.web-ui --no-link
nix build .#checks.x86_64-linux.web-ui --no-link
nix build .#checks.x86_64-linux.ui-screenshots --no-link
nix flake check --keep-going
```

Do not blindly run every command. Confirm the current attribute names with `nix flake show` or repository definitions when needed.

## Verification levels

### Targeted confidence

Use during implementation for isolated logic or a small component. Run formatting and the smallest relevant test selection.

### Feature integration

Use when acceptance criteria cross server/database, UI/API, builder/server, or agent/server boundaries. Start repository services through the devshell's process-compose workflow and exercise the actual path.

### Full Nix integration

Run `nix flake check --keep-going` when:

- The task explicitly requires it.
- Flakes, NixOS modules, devshells, packaging, CI, or release behavior changed.
- Multiple component interfaces changed.
- Targeted verification cannot establish compatibility.
- The task is high risk and preparing for review.

If it is not run, state what was run instead and why that is sufficient. Never imply that the full check passed.

## UI evidence

For a user-visible UI change:

1. Update or add an assertion in the authoritative `web-ui` check when practical.
2. Ensure the check navigates to and renders the changed state.
3. Capture the screenshot from that check.
4. Include it in the MR description or discussion with the tested state identified.

The lightweight `ui-screenshots` helper is useful for iteration but is not a substitute when project policy requires DB-backed `web-ui` evidence.

## Output handling

Large-output summarizers may help locate failures, but verification depends on the original command's exit status. If piping output, enable reliable pipeline status handling or capture the command status separately. Inspect raw output whenever a summary is ambiguous.

When a command fails, report:

- Exact command
- Exit status
- First actionable failure
- Whether it appears caused by the change, the environment, or an unrelated existing failure
- What remains unverified
