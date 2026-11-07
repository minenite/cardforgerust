//! CardForge host internals: plugin manager, event bus, and commands.
//!
//! The Horizon native mod (`crates/cardforge-host`) wires game hooks into
//! [`CardForge`] and registers Minenite plugins against it.

#![deny(missing_docs)]

mod bus;
mod commands;
mod manager;

pub use bus::{EventBus, ListenerId};
pub use commands::{register_builtins, CommandMap, PluginInfo};
pub use manager::{CardForge, LoadError};

pub use cardforge_api as api;
