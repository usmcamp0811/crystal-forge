//! Mock data accessors for systems views.

#[path = "systems_mock_data.rs"]
mod systems_mock_data;

pub use systems_mock_data::{mock_system_detail_by_id, mock_system_details};
