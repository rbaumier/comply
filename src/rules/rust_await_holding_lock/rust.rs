//! rust: covered by `clippy::await_holding_lock`
//!
//! Enable at the crate root:
//!
//! ```ignore
//! #![warn(clippy::await_holding_lock)]
//! ```
//!
//! Holding a `MutexGuard` across an `.await` point causes deadlocks
//! and starvation under tokio's work-stealing scheduler. The guard
//! travels with the future when it's parked on another thread.
//!
//! comply does not run clippy itself.
