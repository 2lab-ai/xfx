//! Shared deterministic test infrastructure.
//!
//! Nothing here talks to a real network endpoint or a real credential. The fake
//! Gateway binds a loopback port, records exactly what xfx sent, and replays a
//! scripted response, so a protocol assertion is about bytes rather than about a
//! mock's expectations.

// Each `tests/*.rs` file is its own crate, so a helper used by only one of them
// is dead code in the others. The module is a shared fixture, not a product
// surface; the alternative is per-crate `cfg` noise that hides real warnings.
#![allow(dead_code)]

pub mod fake_gateway;
