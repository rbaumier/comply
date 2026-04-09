//! rust: covered by `clippy::print_stdout` + `clippy::print_stderr`
//!
//! Enable at the crate root:
//!
//! ```ignore
//! #![warn(clippy::print_stdout, clippy::print_stderr)]
//! ```
//!
//! Library code should never call `println!` / `eprintln!` directly —
//! use the `tracing` crate with structured logging so downstream
//! consumers can configure the output sink.
//!
//! comply does not run clippy itself.
