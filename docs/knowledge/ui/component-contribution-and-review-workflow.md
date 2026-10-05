---
type: UI Design
title: "Frontend Component Contribution and Review Workflow"
description: "Defines the eight-step workflow for adding a reusable web UI component, the PR review checklist, merge-readiness rules, the exception process, and local verification commands."
tags:
  - crystal-forge
  - web-ui
  - components
  - workflow
  - review
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:52-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/frontend-component-standards.md at commit 3b23d36f"
    title: "Frontend Component Isolation Standards"
  - id: code-1
    resource: "Crystal Forge repository file packages/web-ui/src/views/style_guide.rs at commit 3b23d36f"
    title: "style_guide.rs"
  - id: code-2
    resource: "Crystal Forge repository file packages/web-ui/src/showcase/fixtures.rs at commit 3b23d36f"
    title: "fixtures.rs"
  - id: code-3
    resource: "Crystal Forge repository file packages/web-ui/src/showcase/shell.rs at commit 3b23d36f"
    title: "shell.rs"
  - id: code-4
    resource: "Crystal Forge repository file packages/web-ui/Dioxus.toml at commit 3b23d36f"
    title: "Dioxus.toml"
---

# Frontend Component Contribution and Review Workflow

This concept holds the contribution, review, merge-readiness, exception, and local verification sections of the [Frontend Component Isolation Standards](component-isolation-standards.md).

> **Status:** Workflow tooling exists (showcase, fixtures, `/style-guide`). The merge-readiness rule that every Layer 1/2 component has an isolation demo is not met by the existing component set; see [Frontend Component Isolation Standards](component-isolation-standards.md).

## Contribution Workflow

Follow this step-by-step workflow when creating or extracting reusable components.

### Step 1: Extract Component (if applicable)

If extracting from existing page:

1. Identify presentational logic that can be isolated
2. Move component to appropriate layer:
   - Layer 1 (primitives) → `packages/web-ui/src/components/`
   - Layer 2 (composites) → `packages/web-ui/src/components/`
3. Convert data dependencies to props
4. Remove all direct API calls and global state mutations
5. Ensure component is pure/presentational

**Example extraction:**

```rust
// Before (in page container)
div {
    class: "stat-card",
    p { class: "label", "Total Systems" }
    p { class: "value", "{systems.len()}" }
}

// After (extracted component)
#[component]
pub fn StatCard(label: String, value: String, color_class: String) -> Element {
    rsx! {
        div {
            class: "stat-card",
            p { class: "label", "{label}" }
            p { class: "value {color_class}", "{value}" }
        }
    }
}
```

### Step 2: Create Fixtures

Add fixture builder to `packages/web-ui/src/showcase/fixtures.rs`:

```rust
/// Create fixtures for MyComponent showcase demos.
pub fn my_component_fixtures() -> Vec<MyComponentData> {
    vec![
        // Success state
        MyComponentData { /* ... */ },
        // Empty state
        MyComponentData { /* ... */ },
        // Error state (if applicable)
        MyComponentData { /* ... */ },
        // Overflow state
        MyComponentData { /* ... */ },
    ]
}
```

**Requirements:**
- Use deterministic values (`mock_datetime()`, `mock_uuid()`, etc.)
- Cover all required states
- Use realistic but representative data
- Document what each fixture demonstrates

### Step 3: Create Isolation Demo

Add showcase entry to `packages/web-ui/src/views/style_guide.rs`:

```rust
StateMatrix { title: "MyComponent - All States",
    {
        let fixtures = my_component_fixtures();
        rsx! {
            StateTile { label: "success",
                MyComponent { data: fixtures[0].clone() }
            }
            StateTile { label: "empty",
                MyComponent { data: fixtures[1].clone() }
            }
            StateTile { label: "overflow",
                MyComponent { data: fixtures[2].clone() }
            }
        }
    }
}
```

### Step 4: Add Responsive Demo (if needed)

If component layout changes by viewport:

```rust
ResponsiveGrid {
    ResponsivePreview {
        label: "mobile (375px)",
        width_class: MOBILE_WIDTH,
        { rsx! { MyComponent { data: fixture() } } }
    }
    ResponsivePreview {
        label: "desktop (1024px)",
        width_class: DESKTOP_WIDTH,
        { rsx! { MyComponent { data: fixture() } } }
    }
}
```

### Step 5: Integrate Component

Use the component in page containers:

```rust
// In view/page file
use crate::components::MyComponent;

// ...

MyComponent {
    data: some_data_from_state
}
```

### Step 6: Verify Locally

Run the showcase to verify:

```bash
nix develop -c dx serve
```

Navigate to http://localhost:8080/style-guide and verify:
- ✅ Component renders in all states
- ✅ Responsive behavior works (if applicable)
- ✅ No console errors
- ✅ Visual appearance matches expectations

### Step 7: Format and Test

```bash
nix develop -c cargo fmt
nix develop -c cargo clippy -- -D warnings
nix develop -c cargo test
```

### Step 8: Create PR

Open merge request with:
- Clear description of component purpose
- Screenshots of showcase states
- Note any responsive behavior
- Note any accessibility considerations

---

## PR Review Checklist

Use this checklist when reviewing frontend component changes.

### General Component Quality

- [ ] Component is in correct layer (primitive/composite/page)
- [ ] Component interface is props-only (no direct API calls)
- [ ] Component has no hidden dependencies or global state mutations
- [ ] Component follows Dioxus conventions and patterns
- [ ] File is in correct directory (`components/` or `views/`)

### State Coverage

- [ ] Isolation demo exists in `style_guide.rs`
- [ ] Success/default state is shown
- [ ] Loading state is shown (if applicable)
- [ ] Empty state is shown (if applicable)
- [ ] Error state is shown (if applicable)
- [ ] Overflow/long-content state is shown
- [ ] All states use shared fixtures from `fixtures.rs`

### Responsive Behavior

- [ ] Responsive demo exists IF layout changes by viewport
- [ ] Mobile (375px) behavior is shown
- [ ] Desktop (1024px) behavior is shown
- [ ] No layout breaks at any viewport size

### Visual Consistency

- [ ] Component uses theme tokens (not hardcoded colors)
- [ ] Component follows design system patterns
- [ ] Typography is consistent with other components
- [ ] Spacing follows grid system
- [ ] Colors match semantic intent (success=green, error=red, etc.)

### Accessibility

- [ ] Interactive elements are keyboard accessible
- [ ] Focus indicators are visible
- [ ] Semantic HTML is used appropriately
- [ ] ARIA labels exist where needed
- [ ] Color contrast meets WCAG AA (4.5:1 for text, 3:1 for UI)
- [ ] Information not conveyed by color alone

### Code Quality

- [ ] No repeated static inline styles (uses theme tokens instead)
- [ ] No duplicated fixture logic
- [ ] Fixtures are deterministic (no `Utc::now()`, `rand::random()`, etc.)
- [ ] Component is properly documented with doc comments
- [ ] Prop types are clear and well-named

### Testing

- [ ] `cargo fmt` passes
- [ ] `cargo clippy -- -D warnings` passes
- [ ] `cargo test` passes (if tests exist)
- [ ] Showcase renders without console errors

---

## Definition of Merge-Readiness

A reusable component (Layer 1 or Layer 2) is **merge-ready** when ALL of the following are true:

### Hard Requirements (MUST)

1. ✅ Component is prop-driven with no direct API calls
2. ✅ Fixture builder exists in `packages/web-ui/src/showcase/fixtures.rs`
3. ✅ Isolation demo exists in `packages/web-ui/src/views/style_guide.rs`
4. ✅ State matrix shows success, loading, empty, error, and overflow states
5. ✅ Responsive demo exists IF layout changes by viewport
6. ✅ All fixtures are deterministic (no runtime/random values)
7. ✅ Component uses theme tokens (no hardcoded colors/styles)
8. ✅ `cargo fmt` passes
9. ✅ `cargo clippy -- -D warnings` passes
10. ✅ Showcase renders without errors at http://localhost:8080/style-guide

### Soft Requirements (SHOULD)

1. ✅ Component has doc comments explaining purpose and props
2. ✅ Accessibility baseline requirements are met
3. ✅ Component is used in at least one page container
4. ✅ Screenshots included in PR showing showcase states

**Page containers (Layer 3)** do NOT require isolation demos but MUST:
- ✅ Delegate presentation to reusable components
- ✅ Not contain duplicated presentational logic

---

## Exception Process

In rare cases, a component may need to merge without full isolation coverage.

### Valid Exceptions

Exceptions are ONLY allowed for:

1. **Page Containers (Layer 3)** - Never require isolation demos
2. **Temporary scaffolding** - Component will be replaced soon (must have tracking issue/task)
3. **Third-party integration** - Component wraps external library that can't be mocked
4. **Experimental feature** - Component is behind feature flag for testing

### Exception Request Process

To request an exception:

1. **Document reason** in PR description:
   ```markdown
   ## Isolation Exception Request
   
   **Reason:** [Brief explanation]
   **Justification:** [Why isolation is not feasible]
   **Remediation Plan:** [How will this be resolved in future]
   **Tracking Issue:** [Link to issue]
   ```

2. **Get approval** from at least one maintainer

3. **Add TODO comment** in code:
   ```rust
   // TODO(TASK-XXX): Add isolation demo when mock API is available
   ```

4. **Create follow-up backlog task** to add proper isolation coverage

### Exception Denial Criteria

Exceptions will be DENIED for:

- ❌ "Didn't have time" - Not a valid reason
- ❌ "Too hard to mock" - Use fixtures or simplify component
- ❌ "Component is simple" - All reusable components need demos
- ❌ "Only used in one place" - Should still be demonstrable

**General Rule:** If it's worth extracting, it's worth demonstrating.

---

## Local Verification

Use these commands to verify your work before creating a PR.

### Start Development Server

```bash
cd /path/to/crystal-forge
nix develop -c dx serve
```

Navigate to: http://localhost:8080/style-guide

### Run Formatters and Linters

```bash
# Format code
nix develop -c cargo fmt

# Check formatting without modifying
nix develop -c cargo fmt -- --check

# Run Clippy (linter)
nix develop -c cargo clippy --all-targets -- -D warnings
```

### Run Tests

```bash
# Run all tests
nix develop -c cargo test

# Run specific test
nix develop -c cargo test test_name

# Run tests with output
nix develop -c cargo test -- --nocapture
```

### Build for Production

```bash
# Full Nix build (includes all checks)
nix build

# Or from devshell
nix develop -c cargo build --release
```

### View Showcase Paths

All showcase components are located at:

- **Showcase surface:** http://localhost:8080/style-guide
- **Component source:** `packages/web-ui/src/components/`
- **Fixture source:** `packages/web-ui/src/showcase/fixtures.rs`
- **Demo source:** `packages/web-ui/src/views/style_guide.rs`
- **Helper components:** `packages/web-ui/src/showcase/shell.rs`

---

## Summary

Crystal Forge frontend development follows **isolation-first principles**:

1. ✅ All reusable components MUST be prop-driven
2. ✅ All reusable components MUST have isolation demos
3. ✅ All demos MUST use deterministic fixtures
4. ✅ All demos MUST show complete state coverage
5. ✅ Responsive components MUST demonstrate viewport behavior
6. ✅ All components MUST meet accessibility baseline

**Before creating a PR, ask yourself:**

> Can this component be rendered, understood, and validated without running the full application?

If the answer is **no**, the component needs more isolation work.

If the answer is **yes**, you're following Crystal Forge best practices! 🎉

---

**Questions or feedback?** Open an issue or discussion in the Crystal Forge repository.

## Related concepts

- [Frontend Component Isolation Standards](component-isolation-standards.md)
- [Frontend Component Contribution and Review Workflow](component-contribution-and-review-workflow.md)
- [Web UI Coding Standards](web-ui-coding-standards.md)
- [Crystal Forge UI/UX Design System](design-system-overview.md)

## Migration verification notes

- Claim: Workflow files: showcase fixtures, `views/style_guide.rs` demos, `showcase/shell.rs` helpers, `/style-guide` route.
  Finding: All paths exist and the route `/style-guide` is registered (`Route::StyleGuideView`).
  Evidence: views/style_guide.rs; showcase/*; routes.rs
  Case: none
- Claim: Commands `nix develop -c dx serve`, `cargo fmt`, `cargo clippy`, `cargo test`, `nix build`; dev server at localhost:8080.
  Finding: Commands not run (brief forbids builds). Dioxus.toml does not set a port; 8080 is the Dioxus default and was not confirmed. The API client redirects dev-server ports to the backend on port 3000.
  Evidence: packages/web-ui/Dioxus.toml
  Case: not checked
- Claim: Merge-readiness rule 3/4: every Layer 1/2 component has a demo and full state matrix.
  Finding: The rule is a review policy. Existing components are mostly not covered (see component-isolation-standards.md status).
  Evidence: views/style_guide.rs
  Case: implementation incomplete
