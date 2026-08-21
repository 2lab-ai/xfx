//! The agent core: one bounded turn, driven against a [`crate::gateway::Provider`].
//!
//! The agent knows nothing about HTTP and nothing about the terminal. It takes a
//! [`TurnRequest`], a provider, and an event sink, and it guarantees the two
//! properties a caller depends on: assistant text arrives in order, and exactly
//! one terminal event is written.

pub mod machine;
pub mod types;

pub use machine::{allows_step, run_turn, TurnMachine};
pub use types::{TurnError, TurnOutcome, TurnRequest};
