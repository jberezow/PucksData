//! Crate root — re-exports all public modules and the [`AnyError`] type alias.
pub mod api;
pub mod db;
pub mod error;
pub mod fetchers;
pub mod loaders;
pub mod logging;
pub mod models;
pub mod on_ice;
pub mod process;
pub mod provenance;
pub mod replay;
pub mod ui;
pub mod webhooks;

/// Convenience alias for a heap-allocated thread-safe error type.
pub type AnyError = Box<dyn std::error::Error + Send + Sync + 'static>;
