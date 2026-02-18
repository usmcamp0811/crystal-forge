//! Modal dialog components.
//!
//! Provides reusable modal dialogs for confirmations, forms, and other
//! user interactions.

mod confirm_dialog;
mod key_pair_modal;
mod remove_system_dialog;

pub use confirm_dialog::ConfirmDialog;
pub use key_pair_modal::{generate_key_pair, GeneratedKeyPair, KeyPairModal};
pub use remove_system_dialog::RemoveSystemDialog;
