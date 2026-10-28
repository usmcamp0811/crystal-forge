---
type: UI Design
title: "Design System: Component and Interaction Patterns"
description: "Defines button, badge, form input, loading, error, table, and icon patterns, plus keyboard navigation, confirmation dialogs, form validation, and toast rules."
tags:
  - crystal-forge
  - web-ui
  - design-system
  - components
  - interaction
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T21:00:00-05:00
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
    resource: "Crystal Forge repository file packages/web-ui/src/components/notifications/toast.rs at commit 3b23d36f"
    title: "toast.rs"
  - id: code-4
    resource: "Crystal Forge repository file packages/web-ui/src/components/modals/confirm_dialog.rs at commit 3b23d36f"
    title: "confirm_dialog.rs"
  - id: code-5
    resource: "Crystal Forge repository file packages/web-ui/src/components/dialog_focus.rs at commit 3b23d36f"
    title: "dialog_focus.rs"
---

# Design System: Component and Interaction Patterns

This concept holds the Component Patterns and Interaction Patterns sections of the [Crystal Forge UI/UX Design System](design-system-overview.md).

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
// Error toast. The owning view keeps the message in a signal and renders
// the component while the signal holds a value.
Toast {
    message: "Failed to deploy: connection timeout".to_string(),
    is_success: false,
    on_dismiss: move |_| toast.set(None),
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
    title: "Remove System".to_string(),
    description: "Are you sure you want to remove 'atlas-01'? This cannot be undone.".to_string(),
    confirm_label: "Remove".to_string(),
    danger: true,
    on_confirm: move |_| { /* delete */ },
    on_cancel: move |_| { /* close */ },
}
```

`danger: true` styles the confirm button as destructive. `ConfirmDialog` lives in `components/modals/confirm_dialog.rs`.

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

#### Current behavior

`Toast(message, is_success, on_dismiss)` in `components/notifications/toast.rs` shows one toast. The view that owns the toast renders it and clears it in `on_dismiss`, for example `views/system_detail.rs` and `views/cves.rs`.

```rust
if let Some((message, is_success)) = toast.read().clone() {
    Toast {
        message,
        is_success,
        on_dismiss: move |_| toast.set(None),
    }
}
```

- The toast is fixed at the top right with `z-index: 220`.
- A success toast has `role="status"` and `aria-live="polite"`. A failure toast has `role="alert"` and `aria-live="assertive"`.
- No shared toast stack exists. Each view manages its own toast.

#### Target design (not implemented)

The design calls for one shared notification system. The names below, `ToastVariant` and `show_toast`, are the **target API**. They do not exist in the code. Do not copy them into a view.

```rust
// Target API (pseudocode)
show_toast(ToastVariant::Success, "System deployed successfully");
show_toast(ToastVariant::Error, "Deployment failed: {reason}");
show_toast(ToastVariant::Warning, "System is already up to date");
show_toast(ToastVariant::Info, "Syncing flake...");
```

**Target toast rules:**
- Success: Auto-dismiss after 3 seconds
- Error: Persist until dismissed
- Maximum 3 toasts visible
- Stack from bottom-right

---

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

- Claim: Button classes `cf-primary-btn`, `cf-success-btn`, `cf-danger-btn`, `cf-hover-bg`; chips `cf-chip-info`, `cf-chip-warning`, `cf-eval-chip-complete`, `cf-eval-chip-failed`; inputs `cf-input`, `cf-focus-ring`.
  Finding: All classes are defined in app.css; `theme::interactive::PRIMARY_BTN = cf-primary-btn`.
  Evidence: assets/app.css; src/theme.rs
  Case: none
- Claim: Modal focus trap and focus restore on close.
  Finding: Implemented by `DialogFocusRestore` and dialog focus helpers.
  Evidence: components/dialog_focus.rs
  Case: none
- Claim: Loading/skeleton, error, table, icon, validate-on-blur rules.
  Finding: Not checked individually (guidance patterns). Skip links are marked future in the source and were not found.
  Evidence: not checked
  Case: not checked
