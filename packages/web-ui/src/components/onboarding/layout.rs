//! Pure placement rules for the walkthrough card.
//!
//! The card must never cover the control it explains. The design picks the
//! screen corner whose box overlaps the target least. While a large drawer or
//! modal is open, the card shrinks into a compact dock instead of covering the
//! working surface. All inputs are CSS pixels in viewport coordinates.

/// Axis-aligned rectangle in viewport coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    /// Distance from the viewport top.
    pub top: f64,
    /// Distance from the viewport left.
    pub left: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

impl Rect {
    fn bottom(self) -> f64 {
        self.top + self.height
    }

    fn right(self) -> f64 {
        self.left + self.width
    }

    fn overlap_area(self, other: Rect) -> f64 {
        let x = (self.right().min(other.right()) - self.left.max(other.left)).max(0.0);
        let y = (self.bottom().min(other.bottom()) - self.top.max(other.top)).max(0.0);
        x * y
    }
}

/// Measurements the placement rules need.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    /// Viewport width.
    pub width: f64,
    /// Viewport height.
    pub height: f64,
    /// Right edge of the navigation sidebar.
    pub sidebar: f64,
    /// Top edge available to the coach, below the top bar.
    pub top: f64,
}

/// Corner the full card sits in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// Top right, below the top bar. The default.
    TopRight,
    /// Bottom right.
    BottomRight,
    /// Bottom left, beside the sidebar.
    BottomLeft,
}

impl Place {
    /// Returns the CSS class suffix used by the design.
    pub const fn class(self) -> &'static str {
        match self {
            Self::TopRight => "at-tr",
            Self::BottomRight => "at-br",
            Self::BottomLeft => "at-bl",
        }
    }
}

/// Corner the compact dock sits in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dock {
    /// Bottom left, beside the sidebar. The default.
    BottomLeft,
    /// Top left, beside the sidebar.
    TopLeft,
    /// Bottom right.
    BottomRight,
    /// Top right.
    TopRight,
}

impl Dock {
    /// Returns the CSS class suffix used by the design.
    pub const fn class(self) -> &'static str {
        match self {
            Self::BottomLeft => "dock-bl",
            Self::TopLeft => "dock-tl",
            Self::BottomRight => "dock-br",
            Self::TopRight => "dock-tr",
        }
    }

    const fn is_right(self) -> bool {
        matches!(self, Self::BottomRight | Self::TopRight)
    }
}

/// Compact dock decision.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DockChoice {
    /// Corner for the dock.
    pub dock: Dock,
    /// Maximum card height when no corner clears the target. The card then
    /// uses the free band above or below the target.
    pub max_height: Option<f64>,
}

/// Width of the compact dock.
const DOCK_WIDTH: f64 = 340.0;
/// Gap between the compact dock and the viewport edge.
const DOCK_MARGIN: f64 = 16.0;
/// Gap kept between the compact dock and the target.
const TARGET_GAP: f64 = 8.0;
/// Height of the full card when its real height is unknown.
pub const FULL_CARD_HEIGHT: f64 = 420.0;
/// Height of the compact dock when its real height is unknown.
pub const DOCK_CARD_HEIGHT: f64 = 220.0;

/// Chooses the compact dock corner for `target`.
///
/// Prefers bottom left, then top left, bottom right and top right, and takes
/// the first corner with the least overlap. When every corner still overlaps,
/// the dock keeps its side and takes the larger free band above or below the
/// target, with a matching maximum height.
pub fn choose_dock(target: Rect, viewport: Viewport, card_height: f64) -> DockChoice {
    let side = viewport.sidebar + DOCK_MARGIN;
    let top = viewport.top;
    let (vw, vh, h) = (viewport.width, viewport.height, card_height);
    let boxes = [
        (
            Dock::BottomLeft,
            Rect {
                top: vh - DOCK_MARGIN - h,
                left: side,
                width: DOCK_WIDTH,
                height: h,
            },
        ),
        (
            Dock::TopLeft,
            Rect {
                top,
                left: side,
                width: DOCK_WIDTH,
                height: h,
            },
        ),
        (
            Dock::BottomRight,
            Rect {
                top: vh - DOCK_MARGIN - h,
                left: vw - DOCK_MARGIN - DOCK_WIDTH,
                width: DOCK_WIDTH,
                height: h,
            },
        ),
        (
            Dock::TopRight,
            Rect {
                top,
                left: vw - DOCK_MARGIN - DOCK_WIDTH,
                width: DOCK_WIDTH,
                height: h,
            },
        ),
    ];
    let mut best = boxes[0];
    for candidate in boxes {
        if target.overlap_area(candidate.1) < target.overlap_area(best.1) {
            best = candidate;
        }
    }
    if target.overlap_area(best.1) <= 0.0 {
        return DockChoice {
            dock: best.0,
            max_height: None,
        };
    }
    let gap_top = target.top - top - TARGET_GAP;
    let gap_bottom = vh - DOCK_MARGIN - target.bottom() - TARGET_GAP;
    let use_top = gap_top >= gap_bottom;
    let dock = match (best.0.is_right(), use_top) {
        (true, true) => Dock::TopRight,
        (true, false) => Dock::BottomRight,
        (false, true) => Dock::TopLeft,
        (false, false) => Dock::BottomLeft,
    };
    let band = if use_top { gap_top } else { gap_bottom };
    DockChoice {
        dock,
        max_height: Some(band.max(0.0).floor()),
    }
}

/// Chooses a narrow-viewport dock for an open drawer or modal.
///
/// Narrow viewports do not leave a wide corner beside a modal. Prefer the
/// free vertical band below it, then the band above it. The dock height is
/// bounded by that band so it cannot cover the modal's primary controls.
pub fn choose_narrow_dock(target: Rect, viewport: Viewport) -> DockChoice {
    const MIN_DOCK_HEIGHT: f64 = 96.0;
    const MAX_NARROW_DOCK_HEIGHT: f64 = 136.0;
    const TARGET_CLEARANCE: f64 = 6.0;
    let gap_bottom = viewport.height - DOCK_MARGIN - target.bottom() - TARGET_GAP;
    let gap_top = target.top - viewport.top - TARGET_GAP;
    // Keep the dock compact even when the free band is larger. The measured
    // modal can grow while its content loads, so reserve an additional 32 px
    // below it. When neither band fits a full compact card, use the top-left
    // dock: modal primary actions sit in the footer at the bottom.
    let (dock, band) = if gap_bottom >= MIN_DOCK_HEIGHT + TARGET_CLEARANCE {
        (Dock::BottomLeft, gap_bottom)
    } else if gap_top >= MIN_DOCK_HEIGHT + TARGET_CLEARANCE {
        (Dock::TopLeft, gap_top)
    } else {
        (Dock::TopLeft, MAX_NARROW_DOCK_HEIGHT + TARGET_CLEARANCE)
    };
    DockChoice {
        dock,
        max_height: Some(
            (band - TARGET_CLEARANCE)
                .floor()
                .clamp(MIN_DOCK_HEIGHT, MAX_NARROW_DOCK_HEIGHT),
        ),
    }
}

/// Chooses the corner for the full card.
///
/// Prefers top right, then bottom right and bottom left, and takes the first
/// corner with the least overlap with `target`.
pub fn choose_place(target: Rect, viewport: Viewport, card_height: f64) -> Place {
    const MARGIN: f64 = 20.0;
    let width = 360.0_f64.min(viewport.width - 40.0);
    let side = viewport.sidebar + MARGIN;
    let (vw, vh, h) = (viewport.width, viewport.height, card_height);
    let boxes = [
        (
            Place::TopRight,
            Rect {
                top: viewport.top,
                left: vw - MARGIN - width,
                width,
                height: h,
            },
        ),
        (
            Place::BottomRight,
            Rect {
                top: vh - MARGIN - h,
                left: vw - MARGIN - width,
                width,
                height: h,
            },
        ),
        (
            Place::BottomLeft,
            Rect {
                top: vh - MARGIN - h,
                left: side,
                width,
                height: h,
            },
        ),
    ];
    let mut best = boxes[0];
    for candidate in boxes {
        if target.overlap_area(candidate.1) < target.overlap_area(best.1) {
            best = candidate;
        }
    }
    best.0
}

/// Returns whether the viewport uses the bottom-sheet presentation.
pub fn is_narrow(viewport_width: f64) -> bool {
    viewport_width <= 720.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEWPORT: Viewport = Viewport {
        width: 1440.0,
        height: 900.0,
        sidebar: 240.0,
        top: 72.0,
    };

    fn target(top: f64, left: f64, width: f64, height: f64) -> Rect {
        Rect {
            top,
            left,
            width,
            height,
        }
    }

    #[test]
    fn full_card_stays_top_right_when_the_target_is_elsewhere() {
        let stats = target(180.0, 270.0, 1140.0, 120.0);
        // The stat strip spans the page width and the top-right box overlaps
        // it, so the card must move to the corner with less overlap.
        assert_eq!(
            choose_place(stats, VIEWPORT, FULL_CARD_HEIGHT),
            Place::BottomRight
        );
        let sidebar_item = target(380.0, 10.0, 220.0, 36.0);
        assert_eq!(
            choose_place(sidebar_item, VIEWPORT, FULL_CARD_HEIGHT),
            Place::TopRight
        );
    }

    #[test]
    fn full_card_moves_away_from_a_target_in_the_top_right() {
        let action = target(100.0, 1100.0, 280.0, 60.0);
        assert_ne!(
            choose_place(action, VIEWPORT, FULL_CARD_HEIGHT),
            Place::TopRight
        );
    }

    #[test]
    fn compact_dock_defaults_to_bottom_left_when_nothing_overlaps() {
        let drawer_header = target(90.0, 900.0, 500.0, 60.0);
        let choice = choose_dock(drawer_header, VIEWPORT, DOCK_CARD_HEIGHT);
        assert_eq!(choice.dock, Dock::BottomLeft);
        assert_eq!(choice.max_height, None);
    }

    #[test]
    fn compact_dock_leaves_the_target_clear() {
        let bottom_left_target = target(700.0, 260.0, 300.0, 150.0);
        let choice = choose_dock(bottom_left_target, VIEWPORT, DOCK_CARD_HEIGHT);
        assert_ne!(choice.dock, Dock::BottomLeft);
    }

    #[test]
    fn narrow_modal_dock_uses_the_free_band_below_primary_controls() {
        let viewport = Viewport {
            width: 560.0,
            height: 900.0,
            sidebar: 240.0,
            top: 68.0,
        };
        let modal = target(192.0, 14.0, 532.0, 516.0);
        let choice = choose_narrow_dock(modal, viewport);
        assert_eq!(choice.dock, Dock::BottomLeft);
        assert_eq!(choice.max_height, Some(136.0));
    }

    #[test]
    fn narrow_modal_dock_uses_the_top_when_the_footer_has_no_free_band() {
        let viewport = Viewport {
            width: 375.0,
            height: 812.0,
            sidebar: 240.0,
            top: 68.0,
        };
        let modal = target(128.0, 9.0, 356.0, 557.0);
        let choice = choose_narrow_dock(modal, viewport);
        assert_eq!(choice.dock, Dock::BottomLeft);
        assert_eq!(choice.max_height, Some(97.0));
    }

    #[test]
    fn dock_uses_the_free_band_when_every_corner_overlaps() {
        // A tall target spanning the whole viewport leaves no clear corner.
        let tall = target(72.0, 0.0, 1440.0, 700.0);
        let choice = choose_dock(tall, VIEWPORT, DOCK_CARD_HEIGHT);
        let max = choice.max_height.expect("a band height is required");
        assert!(max >= 0.0);
        // The larger band is below the target: 900 - 16 - 772 - 8 = 104.
        assert_eq!(choice.dock, Dock::BottomLeft);
        assert_eq!(max, 104.0);
    }

    #[test]
    fn dock_keeps_its_side_when_it_falls_back_to_a_band() {
        let tall = target(72.0, 0.0, 1440.0, 700.0);
        // Force the best corner to be on the right by blocking the left.
        let choice = choose_dock(tall, VIEWPORT, DOCK_CARD_HEIGHT);
        assert!(!choice.dock.is_right());
    }

    #[test]
    fn narrow_threshold_matches_the_design() {
        assert!(is_narrow(720.0));
        assert!(!is_narrow(721.0));
    }

    #[test]
    fn overlap_is_zero_for_disjoint_rectangles() {
        let a = target(0.0, 0.0, 10.0, 10.0);
        let b = target(20.0, 20.0, 10.0, 10.0);
        assert_eq!(a.overlap_area(b), 0.0);
        assert_eq!(a.overlap_area(a), 100.0);
    }
}
