//! Sortable table header component.

use dioxus::prelude::*;

use crate::theme;

/// Sort direction for table columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDirection {
    /// Ascending order (A-Z, 0-9)
    Asc,
    /// Descending order (Z-A, 9-0)
    Desc,
}

impl SortDirection {
    /// Toggle between ascending and descending.
    pub fn toggle(self) -> Self {
        match self {
            Self::Asc => Self::Desc,
            Self::Desc => Self::Asc,
        }
    }
}

/// A table header cell that supports sorting.
///
/// Shows an arrow indicator when the column is sorted, and toggles
/// between ascending and descending on click. Uses an event handler
/// pattern for flexibility.
///
/// # Example
/// ```ignore
/// let mut sort_column = use_signal(|| None::<MyColumn>);
/// let mut sort_direction = use_signal(|| SortDirection::Asc);
///
/// rsx! {
///     SortableHeader {
///         label: "Name",
///         column: MyColumn::Name,
///         current_col: *sort_column.read(),
///         current_dir: *sort_direction.read(),
///         on_sort: move |(col, dir)| {
///             sort_column.set(Some(col));
///             sort_direction.set(dir);
///         }
///     }
/// }
/// ```
#[component]
pub fn SortableHeader<C: Clone + PartialEq + 'static>(
    /// Display label for the header
    label: &'static str,
    /// The column this header represents
    column: C,
    /// The currently sorted column
    current_col: Option<C>,
    /// Current sort direction
    current_dir: SortDirection,
    /// Callback when sort changes: receives (column, new_direction)
    on_sort: EventHandler<(C, SortDirection)>,
) -> Element {
    let is_active = current_col == Some(column.clone());
    let arrow = if is_active {
        match current_dir {
            SortDirection::Asc => " ▲",
            SortDirection::Desc => " ▼",
        }
    } else {
        ""
    };

    rsx! {
        th {
            class: "px-4 py-3 text-left text-xs font-medium text-gray-400 uppercase tracking-wider cursor-pointer hover:text-white transition-colors",
            onclick: move |_| {
                let new_dir = if is_active {
                    current_dir.toggle()
                } else {
                    SortDirection::Asc
                };
                on_sort.call((column.clone(), new_dir));
            },
            "{label}{arrow}"
        }
    }
}
