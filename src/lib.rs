//! Crate root — re-exports all public modules and the [`AnyError`] type alias.
pub mod api;
pub mod db;
pub mod models;
pub mod fetchers;
pub mod loaders;
pub mod process;
pub mod ui;

pub type AnyError = Box<dyn std::error::Error + Send + Sync + 'static>;
