// src/lib.rs
pub mod api;
pub mod db;

pub type AnyError = Box<dyn std::error::Error + Send + Sync + 'static>;
