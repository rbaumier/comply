//! rust-tracing-subscriber-in-library — installing a global tracing subscriber
//! or logger from library code steals a process-wide decision from the binary.
//!
//! `tracing`'s subscriber and `log`'s logger are both process-global and
//! install-once: whoever calls first wins, every later call is a silent no-op or
//! an error. A library that installs one decides the output format, the filter,
//! and the destination for the whole program — and makes the application's own
//! `init` fail. Libraries emit spans and events; the binary that owns `main`
//! installs the subscriber that collects them.
//!
//! The rule fires on calls that install a process-global collector
//! (`tracing_subscriber::fmt().init()`, `tracing::subscriber::set_global_default(…)`,
//! `env_logger::init()`, `log::set_boxed_logger(…)`, …) reached from library
//! code. Test code, binaries, build scripts and FFI bridge crates are exempt, and
//! so is a `pub` opt-in helper (`pub fn init_tracing()`) the consumer calls
//! deliberately.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-tracing-subscriber-in-library",
    description: "A library emits traces; only the binary installs the global subscriber — a library-side `init()` hijacks the whole process's logging.",
    remediation: "Delete the subscriber/logger installation from the library and emit `tracing::info!` / `tracing::error!` instead; let the consumer's `main` call `tracing_subscriber::fmt().init()` once at startup. If the crate must offer setup, expose it as a `pub fn init_tracing()` the binary calls explicitly.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
