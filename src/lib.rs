// src/lib.rs
pub mod api;
pub mod db;
pub mod models;
pub mod fetchers;
pub mod loaders;

pub type AnyError = Box<dyn std::error::Error + Send + Sync + 'static>;
