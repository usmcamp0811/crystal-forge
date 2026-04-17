---
id: doc-5
title: Crystal Forge UI/UX Design System
type: other
created_date: '2026-03-11 01:38'
---
This document defines the authoritative UI/UX standards for Crystal Forge. All agents implementing UI changes MUST follow these guidelines.

**Document Version:** 1.0  
**Last Updated:** 2026-03-10  
**Applies to:** `packages/web-ui` (Dioxus frontend)

---

## Table of Contents

1. [Design Philosophy](#design-philosophy)
2. [Technology Stack](#technology-stack)
3. [Theming System](#theming-system)
4. [Color System](#color-system)
5. [Typography](#typography)
6. [Spacing & Layout](#spacing--layout)
7. [Component Patterns](#component-patterns)
8. [Interaction Patterns](#interaction-patterns)
9. [Accessibility Requirements](#accessibility-requirements)
10. [Responsive Design](#responsive-design)
11. [Animation & Motion](#animation--motion)
12. [Naming Conventions](#naming-conventions)
13. [Anti-Patterns](#anti-patterns)
14. [Decision Framework](#decision-framework)

---

## Design Philosophy

Crystal Forge follows a **Professional/Enterprise** design philosophy optimized for infrastructure operations teams.

### Core Principles

1. **Clarity over decoration** - Every visual element must serve a functional purpose
2. **Data density with readability** - Show relevant information without overwhelming
3. **Consistent feedback** - Users always know what's happening and what they can do
4. **Keyboard-first** - All actions accessible without a mouse
5. **Dark-first** - Optimized for dark theme; light theme is secondary but fully supported

### Design Goals

| Goal | Implementation |
|------|----------------|
| Scannable dashboards | Status colors, consistent badge placement, clear hierarchy |
| Quick actions | Prominent CTAs, predictable button locations |
| Error visibility | Red indicators, toast notifications, inline feedback |
| Information density | Compact cards, tabular data, collapsible sections |

---

## Technology Stack

| Layer | Technology | Purpose |
|-------|------------|---------|
| Framework | Dioxus 0.7 (Rust/WASM) | Reactive UI components |
| Styling | Tailwind CSS v4 | Utility-first CSS |
| Theming | CSS Custom Properties | Dark/light theme switching |
| Icons | Inline SVG (Heroicons style) | Stroke-based, consistent sizing |
| State | Dioxus Signals + Context | Reactive state management |
| Real-time | WebSocket hooks | Live logs, build status |

### Source of Truth Hierarchy

1. **`packages/web-ui/assets/app.css`** - CSS variables and semantic classes
2. **`packages/web-ui/src/theme.rs`** - Rust constants mapping to CSS classes
3. **This document** - Design rationale and patterns
4. **`/style-guide` route** - Live visual reference

---

## Theming System

Crystal Forge supports both dark and light themes.

- Dark theme is the primary design target.
- Light theme is required and must feel intentional, not like a fallback.
- Every new UI change must be reviewed in both themes.
- Theme support must come from semantic tokens, not per-view overrides.

### Theme Policy

All visual styling must be theme-aware by default.

1. Define shared values in `packages/web-ui/assets/app.css`
2. Expose reusable semantic tokens in `packages/web-ui/src/theme.rs`
3. Consume those tokens in components and views
4. Verify the result in both themes before considering the work complete

### Dark Theme Expectations

Dark theme is optimized for long-running operational use.

- Use deep low-glare backgrounds
- Keep primary content highly legible
- Use accent color sparingly for actions and emphasis
- Let status colors stand out clearly against dark surfaces
- Avoid overly bright borders, fills, and glows

### Light Theme Expectations

Light theme is not a color inversion of dark theme.

- Use soft neutral page backgrounds, not pure white everywhere
- Keep cards and elevated surfaces clearly separated from the page background
- Reduce visual harshness with restrained borders and subtle fills
- Preserve the same hierarchy, density, and semantics as dark theme
- Ensure primary actions remain prominent without overpowering the layout

### Light Theme Design Rules

When implementing or updating UI, light theme must follow these rules:

| Area | Rule |
|------|------|
| Page background | Use a soft app background, not flat white |
| Cards | Use white or near-white cards with visible border separation |
| Text | Use semantic text tokens; never rely on dark-theme `text-white` classes |
| Hover states | Use subtle tinted surfaces, not dark-mode hover carryovers |
| Buttons | Maintain contrast and hierarchy without muddy gray fills |
| Dividers | Use low-contrast borders that still define structure |
| Status chips | Preserve semantic meaning and readable contrast |

### Theme Implementation Rules

```rust
// CORRECT
div { class: "cf-card-bg cf-text-primary border cf-card-border" }

// WRONG - dark-theme assumptions break light theme
div { class: "bg-gray-900 text-white border-gray-700" }
```

```css
/* CORRECT - semantic token */
.cf-card-bg {
  background-color: var(--cf-card-bg);
}

/* WRONG - hardcoded per-theme styling */
.some-card {
  background-color: #111827;
}
```

### Prohibited Theme Practices

Do not do the following except as temporary migration bridges:

- Add new hardcoded `bg-gray-*`, `text-white`, or `border-gray-*` classes in components
- Depend on global `!important` overrides to make light theme readable
- Build components that only look correct in dark theme
- Use different layout structure between themes
- Change semantic meaning between themes

### Theme Verification Checklist

Every UI change should be checked against this list:

- Page background and card surfaces are visually distinct
- Primary, secondary, and muted text remain readable
- Buttons preserve hierarchy in both themes
- Status badges remain semantically correct and legible
- Inputs, dropdowns, and tables render correctly in both themes
- Hover, focus, selected, and disabled states remain visible
- No emergency `!important` override was required for the new change

### Migration Goal

The current repository contains temporary light-theme compatibility overrides in `packages/web-ui/assets/app.css`.

The long-term goal is to remove those overrides by:

1. Replacing hardcoded utility colors with semantic classes
2. Ensuring all components consume theme tokens directly
3. Making light theme a first-class verification target during UI work

---

## Color System

Crystal Forge uses a semantic color system. Colors convey meaning, never decoration.

### Brand Colors

| Token | Dark Mode | Light Mode | Usage |
|-------|-----------|------------|-------|
| `--cf-brand-purple` | `#82699b` | `#654a84` | Primary actions, brand elements |
| `--cf-brand-purple-hover` | `#8616e0` | `#573f72` | Primary button hover |
| `--cf-danger-berry` | `#6f1649` | `#9d2f67` | Destructive actions |

### Semantic Status Colors

Status colors are STRICT. Use ONLY these mappings:

| Status | Color | Tailwind Token | Use Cases |
|--------|-------|----------------|-----------|
| **Success/Healthy** | Emerald | `emerald-400` | Healthy systems, successful builds, up-to-date |
| **Warning** | Amber | `amber-400` | Warning health, behind deploys, draining workers |
| **Error/Critical** | Red | `red-400` | Offline, failed builds, critical health |
| **Neutral/Unknown** | Gray | `gray-500` | Unknown state, never deployed, disabled |
| **Info/In-Progress** | Blue | `blue-400` | Informational, queued, evaluating |

**IMPORTANT:** Do NOT use decorative colors. If a color doesn't map to one of these semantic meanings, it should be gray.

### Surface Colors (Theme-Aware)

These tokens automatically switch between dark and light themes:

| Token | Dark Value | Light Value | Usage |
|-------|------------|-------------|-------|
| `--cf-page-bg` | `#030712` | `#eef2f7` | Page background |
| `--cf-sidebar-bg` | `#111827` | `#ffffff` | Sidebar, elevated surfaces |
| `--cf-card-bg` | `#111827` | `#ffffff` | Card backgrounds |
| `--cf-card-border` | `#1f2937` | `#d1d9e6` | Card borders |
| `--cf-subtle-bg` | `rgba(31,41,55,0.5)` | `#eef3f9` | Table headers, hover states |

### Text Colors (Theme-Aware)

| Token | Dark Value | Light Value | Usage |
|-------|------------|-------------|-------|
| `--cf-text-primary` | `#f3f4f6` | `#1f2937` | Headings, important values |
| `--cf-text-secondary` | `#9ca3af` | `#4b5563` | Labels, descriptions |
| `--cf-text-muted` | `#6b7280` | `#6b7280` | Timestamps, metadata |
| `--cf-text-disabled` | `#4b5563` | `#9ca3af` | Disabled states |

### Color Accessibility Requirements

All color combinations MUST meet WCAG 2.1 AA standards:

| Combination | Minimum Contrast |
|-------------|------------------|
| Normal text on background | 4.5:1 |
| Large text (18px+) on background | 3:1 |
| UI components and graphics | 3:1 |

**Verification:** Use the style guide at `/style-guide` to visually verify color combinations work in both themes.

### Using Colors in Code

```rust
// CORRECT: Use semantic tokens from theme.rs
use crate::theme::{health, text, surface};

div { class: "{health::HEALTHY_TEXT} {health::HEALTHY_BG}" }
p { class: "{text::PRIMARY}" }
div { class: "{surface::CARD_BG}" }

// CORRECT: Use CSS classes from app.css
div { class: "cf-card-bg cf-text-primary" }

// INCORRECT: Hardcoded Tailwind colors
div { class: "bg-gray-900 text-white" }  // NO!
```

---

## Typography

Typography uses Tailwind's default scale with the system font stack.

### Type Scale

| Token | Size | Weight | Usage |
|-------|------|--------|-------|
| `PAGE_TITLE` | `text-2xl` (24px) | `font-bold` | Page headings |
| `SECTION_TITLE` | `text-lg` (18px) | `font-semibold` | Card headers, section titles |
| `STAT_VALUE` | `text-3xl` (30px) | `font-bold` | Dashboard numbers |
| `LABEL` | `text-sm` (14px) | normal | Field labels, descriptions |
| `TABLE_HEADER` | `text-xs` (12px) | `font-medium` | Table column headers |
| `MONO` | `text-sm` (14px) | `font-mono` | Hashes, paths, code |
| `CAPTION` | `text-xs` (12px) | normal | Timestamps, metadata |

### Allowed Typography Classes

To maintain consistency, use ONLY these typography combinations:

```rust
use crate::theme::typography;

h1 { class: "{typography::PAGE_TITLE}" }    // Dashboard, Systems, Builds
h2 { class: "{typography::SECTION_TITLE}" } // Fleet Health, Build Queue
p  { class: "{typography::LABEL}" }         // Field labels
span { class: "{typography::MONO}" }        // /nix/store/abc123...
span { class: "{typography::CAPTION}" }     // 5 minutes ago
```

### Monospace Usage

Use `font-mono` for:
- Git commit SHAs
- Nix store paths
- IP addresses
- Version numbers
- Command output
- Log lines

---

## Spacing & Layout

### Spacing Scale

Crystal Forge uses Tailwind's 4px base unit. These are the approved spacing values:

| Token | Value | Usage |
|-------|-------|-------|
| `gap-1` | 4px | Icon-to-text |
| `gap-2` | 8px | Related items (badge groups) |
| `gap-3` | 12px | Form fields |
| `gap-4` | 16px | Card grid, sections |
| `gap-6` | 24px | Major sections |
| `p-4` | 16px | Small card padding |
| `p-6` | 24px | Standard card padding |
| `p-8` | 32px | Page content padding |

### Container Hierarchy

Layouts follow a strict nesting hierarchy:

```
Page (p-8)
└── Section (gap-6 between sections)
    ├── Section Header (mb-4)
    └── Content Grid (gap-4)
        └── Card (p-6, rounded-xl)
            ├── Card Header (mb-4)
            └── Card Body (gap-3 between items)
```

**RULE:** Never skip levels. A Card must be inside a Section/Grid, never directly in Page.

### Grid System

| Breakpoint | Columns | Usage |
|------------|---------|-------|
| Default | 1 | Mobile |
| `md` (768px) | 2 | Tablets |
| `lg` (1024px) | 3-4 | Desktop |
| `xl` (1280px) | 4+ | Large monitors |

#### Standard Grid Patterns

```rust
// Dashboard widgets (4-column on xl)
div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4" }

// System cards (3-column on lg)
div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4" }

// Two-column content (e.g., form + preview)
div { class: "grid grid-cols-1 lg:grid-cols-2 gap-6" }
```

### Split-Pane Layouts

For master-detail patterns (e.g., Build Queue + Build Detail):

```css
.cf-builds-split {
  display: grid;
  grid-template-columns: minmax(0, 5fr) minmax(0, 7fr);
  gap: 1.5rem;
}

@media (max-width: 1024px) {
  .cf-builds-split {
    grid-template-columns: 1fr;  /* Stack on mobile */
  }
}
```

**Standard Split Ratios:**
- List + Detail: `5fr 7fr` (narrower list)
- Navigation + Content: `4fr 8fr` (sidebar style)
- Equal panels: `1fr 1fr`

### Card Density Guidelines

Cards should be dense but readable. Follow these rules:

| Card Type | Max Content | Structure |
|-----------|-------------|-----------|
| Stat Card | 1 number + 1 label | Large value, small label below |
| Summary Card | 3-5 metrics | Header + 2-column key-value grid |
| List Card | 5-10 items | Header + scrollable list |
| Detail Card | Unlimited | Header + sections with dividers |

**Card Structure Template:**
```rust
div { class: "cf-card-bg border cf-card-border rounded-xl p-6",
    // Header: always present
    div { class: "flex items-center justify-between mb-4",
        h2 { class: "{typography::SECTION_TITLE}", "Card Title" }
        // Optional: action buttons, badges
    }
    // Body: dense content
    div { class: "space-y-3",
        // Content rows
    }
}
```

### When to Use Modals vs Cards vs Inline Forms

| Pattern | Use When |
|---------|----------|
| **Modal** | Destructive actions, multi-step forms, confirmations, focused tasks |
| **Card** | Displaying data, summary information, dashboard widgets |
| **Inline Form** | Quick edits (1-2 fields), toggle settings, search/filter |
| **Side Panel** | Extended details without leaving context, secondary info |
| **Full Page** | Complex forms (5+ fields), wizards, onboarding |

**Decision Flow:**
1. Is it destructive? -> Modal with confirmation
2. Is it a quick toggle or 1-2 fields? -> Inline
3. Does it need context from the page? -> Side panel or inline
4. Is it a complex multi-field form? -> Modal or full page
5. Is it read-only data? -> Card

---

## Component Patterns

### Button Hierarchy

| Type | Class | Usage |
|------|-------|-------|
| Primary | `cf-primary-btn` | Main action per view (Deploy, Save) |
| Success | `cf-success-btn` | Positive confirmations (Confirm, Approve) |
| Danger | `cf-danger-btn` | Destructive actions (Delete, Remove) |
| Ghost | `cf-hover-bg` | Secondary actions, cancel buttons |

**Button Rules:**
- One primary button per visible area
- Danger buttons require confirmation dialog
- Ghost buttons for cancel/dismiss actions
- All buttons must have visible focus state

```rust
// Primary action
button {
    class: "px-4 py-2 rounded-lg text-white font-medium cf-primary-btn cf-focus-ring",
    "Deploy"
}

// Danger action
button {
    class: "px-4 py-2 rounded-lg text-white font-medium cf-danger-btn cf-focus-ring",
    "Remove System"
}
```

### Badge/Chip Patterns

Badges communicate status at a glance:

```rust
// Status badge with dot
div { class: "inline-flex items-center gap-2 px-2.5 py-0.5 rounded-full text-xs font-medium",
    span { class: "w-2 h-2 rounded-full {dot_color}" }
    "{status_label}"
}

// Semantic chip classes
class: "cf-chip-info"     // Blue - informational
class: "cf-chip-warning"  // Amber - warning
class: "cf-eval-chip-complete"  // Teal - success
class: "cf-eval-chip-failed"    // Red - error
```

### Form Inputs

All inputs must use the semantic input class:

```rust
input {
    class: "w-full rounded-lg px-4 py-2 text-sm cf-input cf-focus-ring",
    r#type: "text",
    placeholder: "Search...",
}

// Select/Dropdown
select {
    class: "rounded-lg px-4 py-2 text-sm cf-input cf-focus-ring",
}
```

### Loading States

Use skeleton loaders for initial page load:

```rust
// Skeleton card
div { class: "cf-card-bg border cf-card-border rounded-xl p-6 animate-pulse",
    div { class: "h-4 bg-gray-700 rounded w-1/3 mb-4" }
    div { class: "h-8 bg-gray-700 rounded w-1/2" }
}

// Skeleton table row
tr { class: "animate-pulse",
    td { class: "px-6 py-3",
        div { class: "h-4 bg-gray-700 rounded w-24" }
    }
}
```

**Loading State Rules:**
- Initial page load: Skeleton matching content structure
- Action in progress: Spinner in button, button disabled
- Background refresh: No visible indicator (silent)
- Error recovery: Show last known data + error toast

### Error Handling

```rust
// Error toast (via notification system)
Toast {
    variant: ToastVariant::Error,
    message: "Failed to deploy: connection timeout"
}

// Inline error (forms)
div { class: "text-red-400 text-sm mt-1",
    "Invalid hostname format"
}

// Empty state
div { class: "text-center py-12 cf-text-muted",
    p { "No systems found" }
    p { class: "text-sm mt-2", "Add a system to get started" }
}
```

### Tables

```rust
div { class: "cf-card-bg border cf-card-border rounded-xl overflow-hidden",
    table { class: "w-full",
        thead { class: "cf-subtle-bg",
            tr {
                th { class: "px-6 py-3 text-left {typography::TABLE_HEADER}", "Hostname" }
                th { class: "px-6 py-3 text-left {typography::TABLE_HEADER}", "Status" }
            }
        }
        tbody { class: "divide-y cf-divider",
            // Rows
        }
    }
}
```

### Icon Guidelines

Icons are stroke-based SVG, consistent sizing:

```rust
// Standard icon (navigation, labels)
svg {
    class: "w-4 h-4",  // 16px
    stroke_width: "1.75",
    // SVG path...
}

// Large icon (empty states, features)
svg {
    class: "w-8 h-8",  // 32px
    stroke_width: "1.5",
}
```

**Icon Color Rules:**
- Navigation: `cf-text-secondary`, active: `cf-text-primary`
- Status indicators: Match semantic status color
- Actions: Inherit from parent text color

---

## Interaction Patterns

### Keyboard Navigation

All interactive elements MUST be keyboard accessible:

| Element | Tab | Enter/Space | Escape |
|---------|-----|-------------|--------|
| Button | Focus | Activate | - |
| Link | Focus | Navigate | - |
| Modal | Focus first element | - | Close modal |
| Dropdown | Focus trigger | Open | Close |
| Form field | Focus | Submit (if only field) | - |

**Focus Management:**
- Focus trap in modals (tab cycles within modal)
- Return focus to trigger when modal closes
- Skip links for main content (future)

### Confirmation Dialogs

For destructive actions, always use ConfirmDialog:

```rust
ConfirmDialog {
    title: "Remove System",
    message: "Are you sure you want to remove 'atlas-01'? This cannot be undone.",
    confirm_label: "Remove",
    confirm_variant: ButtonVariant::Danger,
    on_confirm: move |_| { /* delete */ },
    on_cancel: move |_| { /* close */ },
}
```

**Required for:**
- Delete/Remove operations
- Destructive deployments
- Clearing queues
- User role changes

### Form Validation

Validate on blur (when user leaves field):

```rust
input {
    class: "cf-input cf-focus-ring {error_class}",
    onblur: move |_| validate_field(),
}
if !error.is_empty() {
    p { class: "text-red-400 text-sm mt-1", "{error}" }
}
```

### Toast Notifications

```rust
// Success
show_toast(ToastVariant::Success, "System deployed successfully");

// Error
show_toast(ToastVariant::Error, "Deployment failed: {reason}");

// Warning
show_toast(ToastVariant::Warning, "System is already up to date");

// Info
show_toast(ToastVariant::Info, "Syncing flake...");
```

**Toast Rules:**
- Success: Auto-dismiss after 3 seconds
- Error: Persist until dismissed
- Maximum 3 toasts visible
- Stack from bottom-right

---

## Accessibility Requirements

Crystal Forge targets **WCAG 2.1 Level AA** compliance.

### Required ARIA Attributes

```rust
// Buttons with icon-only
button {
    aria_label: "Close modal",
    // icon SVG
}

// Status regions
div {
    role: "status",
    aria_live: "polite",
    // Dynamic content
}

// Form errors
input {
    aria_invalid: "{has_error}",
    aria_describedby: "error-{field_id}",
}
p { id: "error-{field_id}", "{error_message}" }

// Modal
div {
    role: "dialog",
    aria_modal: "true",
    aria_labelledby: "modal-title",
}
```

### Focus Indicators

All interactive elements MUST have visible focus:

```rust
// Use the focus ring class
button { class: "cf-focus-ring", /* ... */ }
input { class: "cf-input cf-focus-ring", /* ... */ }
a { class: "cf-focus-ring", /* ... */ }
```

The `cf-focus-ring` class provides:
```css
.cf-focus-ring:focus,
.cf-focus-ring:focus-visible {
  outline: none;
  box-shadow: 0 0 0 2px var(--cf-focus-ring-color);
}
```

### Screen Reader Considerations

- Use semantic HTML (`button`, `nav`, `main`, `table`)
- Provide `aria-label` for icon-only buttons
- Use `sr-only` class for visually hidden but accessible text
- Announce dynamic changes with `aria-live` regions

---

## Responsive Design

Crystal Forge is **desktop-first** with mobile support.

### Breakpoint System

| Breakpoint | Width | Target |
|------------|-------|--------|
| Default | <768px | Mobile |
| `md` | 768px+ | Tablet |
| `lg` | 1024px+ | Desktop |
| `xl` | 1280px+ | Large desktop |
| `2xl` | 1536px+ | Ultra-wide |

### Mobile Behavior

| Component | Desktop | Mobile |
|-----------|---------|--------|
| Sidebar | Fixed 256px | Hidden (hamburger menu) |
| Grid | 3-4 columns | 1 column |
| Tables | Horizontal scroll | Card view or horizontal scroll |
| Modals | Centered, max-width | Full width, bottom sheet |
| Split panes | Side-by-side | Stacked |

### Z-Index Layering System

| Layer | Z-Index | Usage |
|-------|---------|-------|
| Base | 0 | Normal content |
| Sticky | 10 | Sticky headers |
| Dropdown | 20 | Dropdown menus |
| Sidebar | 30 | Fixed sidebar |
| Modal overlay | 50 | Modal backdrop |
| Modal content | 60 | Modal dialogs |
| Toast | 70 | Notifications |
| Tooltip | 80 | Tooltips |

---

## Animation & Motion

Animation is **subtle and functional**, never decorative.

### Approved Animations

| Animation | Usage | Duration |
|-----------|-------|----------|
| `animate-spin` | Loading spinners | Continuous |
| `transition-colors` | Button hovers | 150ms |
| `transition-opacity` | Fade in/out | 150ms |

### Transitions

```rust
// Button hover
button { class: "transition-colors duration-150 cf-primary-btn hover:..." }

// Modal fade
div { class: "transition-opacity duration-150 {opacity_class}" }
```

### Reduced Motion

Respect `prefers-reduced-motion`:

```css
@media (prefers-reduced-motion: reduce) {
  .animate-spin {
    animation: none;
  }
}
```

---

## Naming Conventions

### CSS Classes

| Type | Prefix | Example |
|------|--------|---------|
| Theme tokens | `cf-` | `cf-card-bg`, `cf-text-primary` |
| Component state | `cf-{component}-{state}` | `cf-eval-chip-pending` |
| Layout helpers | `cf-{pattern}-` | `cf-builds-split` |
| Modifiers | Standard Tailwind | `hover:`, `md:`, `focus:` |

### Rust Components

| Type | Convention | Example |
|------|------------|---------|
| View (page) | `{Name}View` | `DashboardView`, `SystemsView` |
| Component | `PascalCase` | `StatusBadge`, `ConfirmDialog` |
| Hook | `use_{name}` | `use_websocket`, `use_theme` |
| Props | `{Component}Props` | `StatusBadgeProps` |

### File Organization

```
src/
├── views/           # Page-level components
│   └── dashboard.rs
├── components/      # Reusable components
│   ├── layout/      # AppShell, Card, Sidebar
│   ├── forms/       # Input, Select, Button
│   └── status/      # Badges, indicators
├── hooks/           # Custom hooks
├── state/           # Global state
├── api/             # API client, models
└── theme.rs         # Design tokens
```

---

## Anti-Patterns

### DO NOT Do These

#### 1. Hardcoded Tailwind Colors

```rust
// WRONG
div { class: "bg-gray-900 text-white border-gray-700" }

// CORRECT
div { class: "cf-card-bg cf-text-primary border cf-card-border" }
```

#### 2. Inline Styles for Static Values

```rust
// WRONG
div { style: "background: #111827; padding: 24px;" }

// CORRECT
div { class: "cf-card-bg p-6" }
```

#### 3. Inconsistent Spacing

```rust
// WRONG - mixing spacing scales
div { class: "p-5 mb-7 gap-3" }  // 5 and 7 are non-standard

// CORRECT - use 4px scale
div { class: "p-6 mb-8 gap-4" }
```

#### 4. Business Logic in Views

```rust
// WRONG - logic in view
if systems.iter().filter(|s| s.health == "healthy").count() > 5 {
    // render something
}

// CORRECT - compute in adapter/hook
let healthy_count = use_healthy_count(&systems);
```

#### 5. Decorative Colors

```rust
// WRONG - color for aesthetics
span { class: "text-purple-400" }  // Why purple?

// CORRECT - color has meaning
span { class: "{deployment::UP_TO_DATE_TEXT}" }  // Green = good
```

#### 6. Missing Focus States

```rust
// WRONG
button { class: "px-4 py-2 cf-primary-btn", "Click" }

// CORRECT
button { class: "px-4 py-2 cf-primary-btn cf-focus-ring", "Click" }
```

#### 7. Container Hierarchy Violations

```rust
// WRONG - card directly in page
div { class: "p-8",  // Page
    div { class: "cf-card-bg p-6" }  // Card with no grid/section
}

// CORRECT
div { class: "p-8",  // Page
    div { class: "grid grid-cols-2 gap-4",  // Grid
        div { class: "cf-card-bg p-6" }  // Card
    }
}
```

#### 8. Unbounded Content

```rust
// WRONG - text can overflow
p { "{potentially_very_long_path}" }

// CORRECT - truncate with title for full value
p { class: "truncate", title: "{full_path}", "{path}" }

// Or use monospace with overflow
code { class: "font-mono text-sm break-all", "{path}" }
```

---

## Decision Framework

When making UI decisions not explicitly covered by this document:

### 1. Check Existing Patterns First

Look at similar components in the codebase:
- Same domain (builds, systems, flakes)
- Same interaction type (list, detail, form)
- Same data type (status, metrics, logs)

### 2. Consult the Style Guide

Visit `/style-guide` in the running app to see all tokens and patterns visually.

### 3. Apply These Principles

In order of priority:
1. **Consistency** - Match existing patterns
2. **Clarity** - Users understand what they're seeing
3. **Accessibility** - Keyboard and screen reader friendly
4. **Performance** - Minimal DOM, efficient updates

### 4. Document New Patterns

If you create a new pattern:
1. Add it to the style guide view (`views/style_guide.rs`)
2. Add CSS tokens to `assets/app.css` if needed
3. Add Rust constants to `theme.rs` if needed
4. Note in MR that a new pattern was introduced

---

## Visual Reference

For live examples of all components and tokens:

1. Run the development server
2. Navigate to `/style-guide`
3. Toggle light/dark theme to verify both modes

Screenshots are available in `docs/screenshots/` for offline reference.

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-03-10 | Initial design system document |
