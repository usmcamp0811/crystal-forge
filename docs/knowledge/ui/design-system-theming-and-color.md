---
type: UI Design
title: "Design System: Theming and Color System"
description: "Defines the dark and light theme policy, semantic design tokens, status color mapping, surface and text colors, and contrast requirements for the web UI."
tags:
  - crystal-forge
  - web-ui
  - design-system
  - theming
  - color
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:52-05:00
verified:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T08:26:03-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/ui-ux-design-system.md at commit 3b23d36f"
    title: "Crystal Forge UI/UX Design System"
  - id: code-1
    resource: "Crystal Forge repository file packages/web-ui/assets/app.css at commit 3b23d36f"
    title: "app.css"
  - id: code-2
    resource: "Crystal Forge repository file packages/web-ui/src/theme.rs at commit 3b23d36f"
    title: "theme.rs"
  - id: code-3
    resource: "Crystal Forge repository file packages/web-ui/src/state/theme.rs at commit 3b23d36f"
    title: "theme.rs"
---

# Design System: Theming and Color System

This concept holds the Theming System and Color System sections of the [Crystal Forge UI/UX Design System](design-system-overview.md).

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
| `--cf-sidebar-bg` | `#0b1220` | `#ffffff` | Sidebar, elevated surfaces |
| `--cf-card-bg` | `#0f172a` | `#ffffff` | Card backgrounds |
| `--cf-card-border` | `#1f2937` | `#d1d9e6` | Card borders |
| `--cf-subtle-bg` | `rgba(31,41,55,0.5)` | `#eef3f9` | Table headers, hover states |

> **Status:** Resolved in verification. The dark values in the table above now match `packages/web-ui/assets/app.css` (`--cf-sidebar-bg` `#0b1220`, `--cf-card-bg` `#0f172a`). The source document listed `#111827` for both.


### Text Colors (Theme-Aware)

| Token | Dark Value | Light Value | Usage |
|-------|------------|-------------|-------|
| `--cf-text-primary` | `#f3f4f6` | `#1f2937` | Headings, important values |
| `--cf-text-secondary` | `#c1cad7` | `#4b5563` | Labels, descriptions |
| `--cf-text-muted` | `#9aa6b7` | `#6b7280` | Timestamps, metadata |
| `--cf-text-disabled` | `#8793a5` | `#9ca3af` | Disabled states |

> **Status:** Resolved in verification. The dark text values in the table above now match `packages/web-ui/assets/app.css`. The source document listed `#9ca3af`, `#6b7280`, and `#4b5563`.


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

## Related concepts

- [Crystal Forge UI/UX Design System (overview)](design-system-overview.md)
- [Design System: Theming and Color System](design-system-theming-and-color.md)
- [Design System: Typography, Spacing and Layout](design-system-typography-and-layout.md)
- [Design System: Component and Interaction Patterns](design-system-components-and-interaction.md)
- [Design System: Accessibility, Responsive Design and Motion](design-system-accessibility-responsive-motion.md)
- [Design System: Naming Conventions, Anti-Patterns and Decision Framework](design-system-conventions-and-anti-patterns.md)
- [Web UI Coding Standards](web-ui-coding-standards.md)
- [Frontend Component Isolation Standards](component-isolation-standards.md)

## Migration verification notes

- Claim: Dark `--cf-sidebar-bg` and `--cf-card-bg` are `#111827`.
  Finding: Dark values are `#0b1220` (sidebar) and `#0f172a` (card). Table corrected.
  Evidence: assets/app.css `:root[data-theme="dark"]` block
  Case: documentation stale
- Claim: Dark text-secondary/muted/disabled are `#9ca3af`/`#6b7280`/`#4b5563`.
  Finding: Dark values are `#c1cad7`/`#9aa6b7`/`#8793a5`. Table corrected.
  Evidence: assets/app.css `:root, :root[data-theme="dark"]` block
  Case: documentation stale
- Claim: Brand tokens, page-bg, card-border, subtle-bg, light values, light/dark theme selectors.
  Finding: Match the stylesheet (brand purple #82699b/#654a84, hover #8616e0/#573f72, berry #6f1649/#9d2f67; page-bg #030712/#eef2f7; card-border #1f2937/#d1d9e6; subtle-bg rgba(31,41,55,.5)/#eef3f9).
  Evidence: assets/app.css lines 22-200
  Case: none
- Claim: The repository contains temporary light-theme compatibility overrides (migration goal).
  Finding: Still true: `:root[data-theme="light"] .text-white`, `.bg-gray-900` and similar overrides exist; the removal goal is not met.
  Evidence: assets/app.css lines ~521-550
  Case: implementation incomplete (migration goal stays open; partial status retained)
- Claim: Status colors map to Tailwind emerald/amber/red/gray/blue 400/500 via theme.rs.
  Finding: Confirmed: health, deployment modules use emerald-400, amber-400, red-400, gray-500, blue-400.
  Evidence: src/theme.rs modules health/deployment
  Case: none
- Claim: Theme state is persisted and applied with data-theme.
  Finding: Theme state toggles dark/light and persists under `cf.ui.theme`.
  Evidence: src/state/theme.rs
  Case: none
