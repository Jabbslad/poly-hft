//! WebSocket client library
//!
//! Provides a reusable WebSocket client with automatic reconnection,
//! ping/pong handling, and configurable backoff.

mod client;
mod types;

pub use client::WsClient;
pub use types::{WsConfig, WsError, WsMessage};
