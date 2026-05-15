//! Transport abstraction for sync.
//! The reference scenario uses in-process sync (no network needed for the benchmark).
//! This module provides the interface for TCP/WebSocket transport in production deployments.

pub mod peer_session;
pub mod transport;

pub use peer_session::PeerSession;
pub use transport::{InProcessTransport, Transport};
