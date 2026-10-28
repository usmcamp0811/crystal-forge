---
type: UI Design
title: "Frontend Component Isolation Standards"
description: "Defines the component taxonomy, required state coverage, fixture conventions, responsive verification, and accessibility baseline for isolation-driven web UI components."
tags:
  - crystal-forge
  - web-ui
  - components
  - showcase
  - standards
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
    resource: "Crystal Forge repository file packages/web-ui/src/components/status_badge.rs at commit 3b23d36f"
    title: "status_badge.rs"
---

# Frontend Component Isolation Standards

**Version:** 1.0  
**Last Updated:** 2026-03-17  
**Status:** Active

> **Status:** Implementation incomplete relative to this standard. The tooling exists: `packages/web-ui/src/showcase/{fixtures,shell}.rs` and the `/style-guide` route (`views/style_guide.rs`). Coverage does not: `style_guide.rs` demonstrates about 12 components (for example `HealthBadge`, `DeploymentBadge`, `StatCard`, `SystemCard`, `BuildQueueRow`, `DonutChartWithLegend`, `ViewToggle`, `BuildSummaryPanel`), while `src/components` has about 190 `#[component]` functions. Most reusable components have no isolation demo, and the required state matrix (success/loading/empty/error/overflow) is not shown for all demonstrated components.

> **Related Documentation:**
> - [Web UI Coding Standards](./web-ui-coding-standards.md) - Styling and theme token policies
> - [UI/UX Design System](design-system-overview.md) - Design philosophy and patterns
> - [Frontend Views Specification](frontend-navigation-and-shared-patterns.md) - View-level architecture

## Table of Contents

> **Status:** This document was split into two concepts. Each entry below links to the concept that now holds the section.

1. [Why Isolation-Driven Development](#why-isolation-driven-development)
2. [Component Taxonomy](#component-taxonomy)
3. [Required State Coverage](#required-state-coverage)
4. [Fixture Conventions](#fixture-conventions)
5. [Responsive Verification](#responsive-verification)
6. [Accessibility Baseline](#accessibility-baseline)
7. [Contribution Workflow](component-contribution-and-review-workflow.md#contribution-workflow)
8. [PR Review Checklist](component-contribution-and-review-workflow.md#pr-review-checklist)
9. [Definition of Merge-Readiness](component-contribution-and-review-workflow.md#definition-of-merge-readiness)
10. [Exception Process](component-contribution-and-review-workflow.md#exception-process)
11. [Local Verification](component-contribution-and-review-workflow.md#local-verification)

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

## Related concepts

- [Frontend Component Isolation Standards](component-isolation-standards.md)
- [Frontend Component Contribution and Review Workflow](component-contribution-and-review-workflow.md)
- [Web UI Coding Standards](web-ui-coding-standards.md)
- [Crystal Forge UI/UX Design System](design-system-overview.md)

## Migration verification notes

- Claim: All Layer 1/2 components must have isolation demos with complete state matrices.
  Finding: Only a subset is covered by the style guide (about 12 components of about 190 `#[component]` functions).
  Evidence: views/style_guide.rs; src/components
  Case: implementation incomplete
- Claim: Showcase helpers `StateMatrix`, `StateTile`, `ResponsiveGrid`, `ResponsivePreview`, widths 375/768/1024/1440 px.
  Finding: Confirmed in shell.rs: `MOBILE_WIDTH` `max-w-[375px]`, `TABLET_WIDTH` 768, `DESKTOP_WIDTH` 1024, `WIDE_WIDTH` 1440; `StateMatrix(title: &'static str)`, `StateTile(label: &'static str)`.
  Evidence: showcase/shell.rs
  Case: none
- Claim: Fixtures are centralized, typed, deterministic (`mock_datetime`, `mock_uuid(index: u8)`).
  Finding: Confirmed: fixed `2026-03-16T12:00:00Z`, `mock_uuid(index: u8)`, `system_summary_fixtures`, `build_queue_item_fixtures`; no `Utc::now` or random values in fixtures.rs. The helpers are private (`fn`), not `pub`.
  Evidence: showcase/fixtures.rs
  Case: none
- Claim: Examples: primitives `HealthBadge`, `DeploymentBadge`, `StatCard`; composites `SystemCard`, `BuildQueueRow`, `DonutChartWithLegend`; containers `DashboardView`, `SystemsListView`, `BuildsView`.
  Finding: All exist (`components/status_badge.rs`, `components/stat_card.rs`, `components/dashboard/build_queue.rs`, `components/charts/donut.rs`, `views/dashboard.rs`, `views/systems_list.rs`, `views/builds.rs`). `SystemsListView` is a sub-view rendered by `SystemsView`.
  Evidence: src/components; src/views
  Case: none
- Claim: Accessibility baseline items (keyboard, ARIA, contrast).
  Finding: Normative requirements; not checked per component.
  Evidence: not checked
  Case: not checked
