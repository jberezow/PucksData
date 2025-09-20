pub mod api;
pub mod cache;
pub mod cli_builder;
pub mod endpoints;
pub mod ingest;
pub mod models;
pub mod storage;
pub mod workflows;

pub type AnyError = Box<dyn std::error::Error + Send + Sync + 'static>;
