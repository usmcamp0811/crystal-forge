---
type: UI Design
title: "Design System: Accessibility, Responsive Design and Motion"
description: "Defines the WCAG 2.1 AA accessibility requirements, ARIA and focus rules, breakpoint and mobile behavior, z-index layers, and approved animations for the web UI."
tags:
  - crystal-forge
  - web-ui
  - design-system
  - accessibility
  - responsive
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:52-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/ui-ux-design-system.md at commit 3b23d36f"
    title: "Crystal Forge UI/UX Design System"
  - id: code-1
    resource: "Crystal Forge repository file packages/web-ui/assets/app.css at commit 3b23d36f"
    title: "app.css"
  - id: code-2
    resource: "Crystal Forge repository file packages/web-ui/src/components/layout/sidebar.rs at commit 3b23d36f"
    title: "sidebar.rs"
  - id: code-3
    resource: "Crystal Forge repository file packages/web-ui/src/components/layout/topbar.rs at commit 3b23d36f"
    title: "topbar.rs"
  - id: code-4
    resource: "Crystal Forge repository file packages/web-ui/src/components/notifications/toast.rs at commit 3b23d36f"
    title: "toast.rs"
---

# Design System: Accessibility, Responsive Design and Motion

This concept holds the Accessibility Requirements, Responsive Design, and Animation & Motion sections of the [Crystal Forge UI/UX Design System](design-system-overview.md).

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

> **Status:** Documentation stale, resolved in verification. `packages/web-ui/assets/app.css` defines `.cf-focus-ring` with `box-shadow: 0 0 0 2px var(--cf-focus-ring)`. The custom property is named `--cf-focus-ring`, not `--cf-focus-ring-color` as shown above.
>
> **Status:** Implementation differs from the z-index table. `app.css` and component classes use other layers (for example toast `z-index: 220`, modal overlays `z-50`, and values of 180-240 for overlays and popovers); the 0/10/20/30/50/60/70/80 scale is not enforced. Sidebar width is `16rem`/`4rem` (collapsed), not a fixed 256px; below 768px the sidebar is replaced by a mobile drawer opened from the topbar (`aria-label` "Open navigation menu").


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

- Claim: `.cf-focus-ring` uses `var(--cf-focus-ring-color)`.
  Finding: Code uses `var(--cf-focus-ring)`.
  Evidence: assets/app.css lines 471-475
  Case: documentation stale (noted in status block; code snippet left as source text)
- Claim: Z-index layering table (toast 70, tooltip 80, modal 50/60).
  Finding: Code uses a broader set of values (e.g. toast 220, overlays 180-240).
  Evidence: assets/app.css z-index declarations; toast.rs
  Case: implementation incomplete (scale not enforced)
- Claim: Mobile: sidebar hidden with hamburger menu; <768px.
  Finding: Mobile drawer in sidebar.rs (`is_mobile_drawer_open`) toggled from topbar.rs.
  Evidence: components/layout/sidebar.rs, topbar.rs
  Case: none
- Claim: Reduced motion rule for `.animate-spin`.
  Finding: app.css has `prefers-reduced-motion` blocks for selected animations (attention flash, coach, others) but none targeting `.animate-spin` (defined in tailwind.css).
  Evidence: assets/app.css lines 2017, 3825, 6918, 6946; tailwind.css:361
  Case: implementation incomplete
- Claim: Toast uses aria-live; modal uses role/aria-modal; `.sr-only` exists.
  Finding: Toast uses `role` status/alert with `aria_live` polite/assertive; `.sr-only` defined at app.css:817. Other ARIA claims not checked per component.
  Evidence: toast.rs; assets/app.css
  Case: partially checked
