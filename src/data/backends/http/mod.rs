//! Shared HTTP network transport utilities for WebAssembly and desktop targets.

pub mod fetch;

pub use fetch::{fetch_url_byte_range, fetch_url_bytes};
