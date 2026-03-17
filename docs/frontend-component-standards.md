# Frontend Component Isolation Standards

**Version:** 1.0  
**Last Updated:** 2026-03-17  
**Status:** Active

> **Related Documentation:**
> - [Web UI Coding Standards](./web-ui-coding-standards.md) - Styling and theme token policies
> - [UI/UX Design System](./ui-ux-design-system.md) - Design philosophy and patterns
> - [Frontend Views Specification](./specs/01-frontend-views.md) - View-level architecture

---

## Table of Contents

1. [Why Isolation-Driven Development](#why-isolation-driven-development)
2. [Component Taxonomy](#component-taxonomy)
3. [Required State Coverage](#required-state-coverage)
4. [Fixture Conventions](#fixture-conventions)
5. [Responsive Verification](#responsive-verification)
6. [Accessibility Baseline](#accessibility-baseline)
7. [Contribution Workflow](#contribution-workflow)
8. [PR Review Checklist](#pr-review-checklist)
9. [Definition of Merge-Readiness](#definition-of-merge-readiness)
10. [Exception Process](#exception-process)
11. [Local Verification](#local-verification)

---

## Why Isolation-Driven Development

Crystal Forge frontend components must be **validated in isolation** to ensure:

### Benefits

1. **Faster iteration** - Develop components without waiting for full application builds
2. **Complete state coverage** - Verify all edge cases (loading, empty, error, overflow) systematically
3. **Visual regression prevention** - Catch UI breaks before they reach production
4. **Reusability confidence** - Isolated components are inherently more reusable
5. **Documentation by demonstration** - Showcase serves as living component documentation

### Core Principle

> **A component that cannot be rendered in isolation has hidden dependencies and is not truly reusable.**

All reusable components in Crystal Forge **must** have:
- Props-only data dependencies (no direct API calls)
- Deterministic fixture-based demos
- Complete state matrix coverage

---

## Component Taxonomy

Crystal Forge frontend components are classified into three layers with distinct responsibilities and requirements.

### Layer 1: Primitives

**Definition:** Small, stateless, single-purpose UI building blocks.

**Examples:**
- `HealthBadge` - Status indicator showing Healthy/Warning/Critical/Offline
- `DeploymentBadge` - Deployment status indicator
- `StatCard` - Simple metric display card

**Characteristics:**
- ✅ Props-only interface
- ✅ No business logic
- ✅ No API calls
- ✅ Highly reusable across views
- ✅ Small file size (<100 lines typically)

**Location:** `packages/web-ui/src/components/`

**Requirements:**
- MUST have isolation demo
- MUST show all visual states
- SHOULD be responsive-aware

---

### Layer 2: Composites

**Definition:** Combination of primitives and other composites to create complex, reusable widgets.

**Examples:**
- `SystemCard` - System summary card combining badges, metrics, and actions
- `BuildQueueRow` - Build queue item combining status, metadata, and progress
- `DonutChartWithLegend` - Data visualization with interactive legend

**Characteristics:**
- ✅ Props-only interface
- ✅ Composes multiple primitives/components
- ✅ May include local UI state (hover, expand/collapse)
- ✅ No API calls or global state mutation
- ✅ Moderate complexity (100-300 lines typically)

**Location:** `packages/web-ui/src/components/`

**Requirements:**
- MUST have isolation demo
- MUST show all data states (loading, empty, success, error, overflow)
- MUST show responsive behavior if layout varies by viewport
- MUST use shared fixtures (no ad-hoc fixture blobs)

---

### Layer 3: Page Containers

**Definition:** View-level components that orchestrate data fetching, state management, and layout.

**Examples:**
- `DashboardView` - Main dashboard page
- `SystemsListView` - Systems management page
- `BuildsView` - Build queue control center

**Characteristics:**
- ❌ Directly calls APIs and manages loading states
- ❌ Contains business logic and data orchestration
- ❌ May use global state (context, signals)
- ✅ Composes Layer 1 and Layer 2 components
- ✅ Large file size (300+ lines typically)

**Location:** `packages/web-ui/src/views/`

**Requirements:**
- ❌ Isolation demos NOT required (page-level testing instead)
- ✅ MUST delegate presentation to reusable components
- ✅ MUST NOT contain presentational logic that should be extracted

**Extraction Rule:**
> If a presentational pattern appears in 2+ page containers, it MUST be extracted to Layer 1 or Layer 2.

---

## Required State Coverage

All **Layer 1 and Layer 2** components must demonstrate the following states in their isolation demos:

### Mandatory States

| State | Description | Example |
|-------|-------------|---------|
| **Success/Default** | Normal happy-path rendering with typical data | System card showing healthy production server |
| **Loading** | Component rendered while data is being fetched | Skeleton UI, spinners, or placeholder content |
| **Empty** | Component rendered with no data available | Empty list, "No results found" message |
| **Error** | Component rendered when data fetch failed | Error message, retry button |
| **Overflow** | Component with extremely long content | Long hostnames, multi-line commit messages, large numbers |

### Conditional States

| State | When Required | Example |
|-------|---------------|---------|
| **Permission-Limited** | Component shows/hides features based on roles | Admin-only actions hidden for viewer role |
| **Disabled** | Component supports disabled state | Disabled button, read-only form field |
| **Interactive States** | Component has hover/focus/active states | Button hover effects, dropdown open/closed |

### State Matrix Requirements

Isolation demos MUST use `StateMatrix` and `StateTile` components:

```rust
StateMatrix { title: "ComponentName - All States",
    {
        rsx! {
            StateTile { label: "success",
                MyComponent { data: success_fixture() }
            }
            StateTile { label: "loading",
                MyComponent { data: loading_fixture() }
            }
            StateTile { label: "empty",
                MyComponent { data: empty_fixture() }
            }
            StateTile { label: "error",
                MyComponent { data: error_fixture() }
            }
            StateTile { label: "overflow",
                MyComponent { data: overflow_fixture() }
            }
        }
    }
}
```

---

## Fixture Conventions

Crystal Forge uses **typed fixture builders** to ensure consistent, deterministic demo data.

### Fixture Location

**File:** `packages/web-ui/src/showcase/fixtures.rs`

All fixtures MUST be defined in this centralized file to prevent duplication.

### Fixture Structure

Fixtures MUST use typed builders returning actual API model types:

```rust
/// Create SystemSummary fixtures for showcase demos with all states.
pub fn system_summary_fixtures() -> Vec<SystemSummary> {
    let base_time = mock_datetime();
    
    vec![
        // Success state
        SystemSummary {
            id: mock_uuid(1),
            hostname: "web-server-1".to_string(),
            health_status: HealthStatus::Healthy,
            // ... other fields
        },
        // Error state
        SystemSummary {
            id: mock_uuid(2),
            hostname: "db-primary".to_string(),
            health_status: HealthStatus::Critical,
            // ... other fields
        },
        // Overflow state
        SystemSummary {
            id: mock_uuid(3),
            hostname: "production-worker-node-with-very-long-hostname-01".to_string(),
            // ... other fields
        },
    ]
}
```

### Fixture Naming Convention

| Pattern | Usage |
|---------|-------|
| `{model}_fixtures()` | Returns `Vec<Model>` with multiple states |
| `{model}_fixture()` | Returns single `Model` instance |
| `mock_{helper}()` | Helper for deterministic values (dates, UUIDs, etc.) |

**Examples:**
- `system_summary_fixtures()` → `Vec<SystemSummary>`
- `build_queue_item_fixtures()` → `Vec<BuildQueueItem>`
- `mock_datetime()` → `DateTime<Utc>`
- `mock_uuid(index: u8)` → `Uuid`

### Determinism Requirement

Fixtures MUST be **deterministic** (same output every time):

✅ **Good:**
```rust
fn mock_datetime() -> DateTime<Utc> {
    "2026-03-16T12:00:00Z".parse().unwrap()
}
```

❌ **Bad:**
```rust
fn mock_datetime() -> DateTime<Utc> {
    Utc::now() // Non-deterministic!
}
```

### Anti-Patterns

❌ **NEVER:**
- Inline fixture data directly in showcase components
- Duplicate fixture logic across multiple demos
- Use random or time-based fixture data
- Import from production code paths for fixtures

---

## Responsive Verification

Components with layout changes across viewports MUST demonstrate responsive behavior.

### Viewport Breakpoints

Crystal Forge uses these standard breakpoints (defined in `packages/web-ui/src/showcase/shell.rs`):

| Constant | Width | Usage |
|----------|-------|-------|
| `MOBILE_WIDTH` | 375px | Small mobile phones |
| `TABLET_WIDTH` | 768px | Tablets and large phones |
| `DESKTOP_WIDTH` | 1024px | Desktop and laptop screens |
| `WIDE_WIDTH` | 1440px | Wide desktop monitors |

### Responsive Demo Requirements

If a component's **layout, truncation, or grid changes** at different widths, it MUST include a responsive demo:

```rust
ResponsiveGrid {
    ResponsivePreview {
        label: "mobile (375px)",
        width_class: MOBILE_WIDTH,
        {
            rsx! {
                // Mobile layout (single column, stacked, etc.)
                MyComponent { data: fixture() }
            }
        }
    }
    ResponsivePreview {
        label: "desktop (1024px)",
        width_class: DESKTOP_WIDTH,
        {
            rsx! {
                // Desktop layout (multi-column grid, expanded, etc.)
                MyComponent { data: fixture() }
            }
        }
    }
}
```

### When Responsive Demos Are Required

| Scenario | Responsive Demo Required? |
|----------|--------------------------|
| Component uses CSS grid that changes columns by viewport | ✅ Yes |
| Component truncates text differently on mobile vs desktop | ✅ Yes |
| Component shows/hides elements based on screen size | ✅ Yes |
| Component is always full-width with no layout changes | ❌ No |
| Component is always fixed-size (e.g., icon badge) | ❌ No |

---

## Accessibility Baseline

All components MUST meet these minimum accessibility requirements:

### Keyboard Navigation

- ✅ All interactive elements MUST be keyboard accessible
- ✅ Tab order MUST follow visual flow
- ✅ Focus indicators MUST be visible

### Semantic HTML

- ✅ Use semantic HTML elements (`<button>`, `<nav>`, `<article>`, etc.)
- ✅ Headings MUST follow proper hierarchy (h1 → h2 → h3)
- ✅ Links MUST have descriptive text (not "click here")

### ARIA Labels

- ✅ Interactive elements without visible text MUST have `aria-label`
- ✅ Complex widgets SHOULD use appropriate ARIA roles
- ✅ Loading states MUST include `aria-live` regions

### Color Contrast

- ✅ Text MUST meet WCAG AA contrast requirements (4.5:1 for normal text)
- ✅ Interactive elements MUST meet WCAG AA contrast (3:1 for large text)
- ✅ Do not rely on color alone to convey information

### Accessibility Review Checklist

When reviewing component isolation demos:

- [ ] Can all actions be performed with keyboard only?
- [ ] Are focus indicators clearly visible?
- [ ] Are interactive elements properly labeled?
- [ ] Does the component work with screen readers? (manual test if possible)
- [ ] Is color contrast sufficient in both light and dark themes?

---

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
2. **Temporary scaffolding** - Component will be replaced soon (must have GitHub issue)
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
   // TODO(ISSUE-123): Add isolation demo when mock API is available
   ```

4. **Create follow-up task** to add proper isolation coverage

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
nix develop
dx serve
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
