//! Table components for list views.
//!
//! Provides sortable headers, table containers, and other table-related
//! reusable components.

mod sortable_header;
mod systems_table;

pub use sortable_header::{SortDirection, SortableHeader};
pub use systems_table::{SystemsSortColumn, SystemsTable};
