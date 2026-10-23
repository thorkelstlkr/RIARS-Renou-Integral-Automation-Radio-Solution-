//! Automation: one-off scheduled events, and long-term programming (categories,
//! format clocks, the dayparts grid and the playlist generator).

pub mod events;
pub mod generator;
pub mod grid;
pub mod model;
pub mod runner;
pub mod store;
pub mod traffic_store;
pub mod traffic;

pub use events::{run_due, scheduler_loop, Fired, StreamCommand};
pub use generator::{Catalog, Generator};
pub use runner::{tick, Automation, TickReport};
