//! Systems domain module.
//!
//! Provides the adapter layer for loading systems data from the backend API,
//! with deterministic mock fallback when the backend is unavailable.
//!
//! Architecture:
//! - `adapter` — fetches from backend, handles auth redirect and fallback logic
//!
//! HTTP calls are in [`crate::api::client`].
//! DTOs are in [`crate::api::models`].
//! Views consume [`adapter`] only; they MUST NOT call the API client directly.

pub mod adapter;
