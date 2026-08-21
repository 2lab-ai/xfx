//! The agent core: one bounded turn, driven against a [`crate::gateway::Provider`].
//!
//! The agent knows nothing about HTTP, nothing about the terminal, and nothing
//! about files. It takes a [`TurnRequest`], a provider, an event sink, and a
//! [`TurnJournal`], and it guarantees the three properties a caller depends on:
//! assistant text arrives in order, exactly one terminal event is written, and
//! exactly one conclusion is journaled.

pub mod machine;
pub mod types;

pub use machine::{allows_step, run_turn, run_turn_saved, TurnMachine};
pub use types::{NoJournal, TurnError, TurnJournal, TurnOutcome, TurnRequest};
