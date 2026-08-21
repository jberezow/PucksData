//! Crate root — re-exports all public modules and the [`AnyError`] type alias.
pub mod api;
pub mod db;
pub mod fetchers;
pub mod loaders;
pub mod models;
pub mod process;
pub mod ui;

/// Convenience alias for a heap-allocated thread-safe error type.
pub type AnyError = Box<dyn std::error::Error + Send + Sync + 'static>;
