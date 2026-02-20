//! Modal dialog components.
//!
//! Provides reusable modal dialogs for confirmations, forms, and other
//! user interactions.

mod confirm_dialog;
mod key_pair_modal;
mod remove_system_dialog;
mod rollback_confirm_dialog;
mod sync_confirm_dialog;

pub use confirm_dialog::ConfirmDialog;
pub use key_pair_modal::{GeneratedKeyPair, KeyPairModal, generate_key_pair};
pub use remove_system_dialog::RemoveSystemDialog;
pub use rollback_confirm_dialog::RollbackConfirmDialog;
pub use sync_confirm_dialog::SyncConfirmDialog;
