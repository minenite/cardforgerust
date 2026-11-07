//! CardForge host internals: plugin manager and typed event bus.
//!
//! The Horizon native mod (`crates/cardforge-host`) wires game hooks into
//! [`CardForge`] and registers Minenite plugins against it.

#![deny(missing_docs)]

mod bus;
mod manager;

pub use bus::{EventBus, ListenerId};
pub use manager::{CardForge, LoadError};

pub use cardforge_api as api;
