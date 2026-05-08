---
id: TASK-293
title: Add non-interactive flag to cf-keygen for automated key generation
status: In Progress
assignee: []
created_date: '2026-05-08 03:12'
updated_date: '2026-05-08 03:41'
labels:
  - enhancement
  - blocker
  - cli
dependencies:
  - TASK-292
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The `cf-keygen` binary has an interactive confirmation prompt that blocks in automated scripts:

```rust
eprint!("📝 Save key to {}? [Y/n] ", path.display());
io::stdin().read_line(&mut input).unwrap();
```

This prevents the NixOS module from automatically generating builder API keys in preStart scripts.

## Impact

- Blocks TASK-292 (builder API mode support in NixOS module)
- Forces users to manually run cf-keygen or use fragile workarounds like `echo y | cf-keygen`
- Prevents automated deployment pipelines from generating keys

## Desired Outcome

Add a `-y` or `--yes` flag to cf-keygen that skips the interactive confirmation prompt.

**Usage:**
```bash
# Interactive (current behavior)
cf-keygen -f /var/lib/crystal-forge/builder-api.key

# Non-interactive (new)
cf-keygen -y -f /var/lib/crystal-forge/builder-api.key
```

## Implementation

**File:** `packages/default/src/bin/cf-keygen.rs`

**Changes:**

1. Add `-y, --yes` option to argument parser (around line 44-75)
2. Add a boolean flag `skip_confirm` to track if `-y` was passed
3. Conditionally skip the confirmation prompt (lines 84-92) if `skip_confirm` is true
4. Update help text to document the new flag

**Example code:**
```rust
let mut skip_confirm = false;

// In argument parser:
"-y" | "--yes" => {
    skip_confirm = true;
}

// Replace confirmation section:
if !skip_confirm {
    eprint!("📝 Save key to {}? [Y/n] ", path.display());
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let trimmed = input.trim();
    if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("y") {
        eprintln!("Aborted.");
        process::exit(0);
    }
}
```

## Acceptance Criteria

- [ ] Add `-y` and `--yes` flags to cf-keygen argument parser
- [ ] Skip confirmation prompt when `-y` or `--yes` is provided
- [ ] Update help text to document the new flags
- [ ] Test interactive mode still works (default behavior unchanged)
- [ ] Test non-interactive mode generates keys without prompting
- [ ] Update any documentation that references cf-keygen usage
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: claude-agent on gray in ~/code/crystal-forge/dev
<!-- SECTION:NOTES:END -->
