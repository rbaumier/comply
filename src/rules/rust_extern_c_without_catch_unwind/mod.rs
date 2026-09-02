//! rust-extern-c-without-catch-unwind — a panic that reaches an `extern "C"`
//! boundary aborts the process.
//!
//! The `"C"` ABI has no notion of unwinding, so Rust cannot let a panic cross
//! it. Since 1.81 the runtime aborts the process on the spot: the C caller
//! never regains control, its destructors never run, and the crash report is a
//! bare `SIGABRT`. A callback handed to a C library — a signal handler, a
//! comparator, a plugin entry point — is exactly where an `unwrap()` or an
//! out-of-bounds index eventually panics.
//!
//! The rule fires on `extern "C"` function *definitions* whose body can panic
//! and never calls `std::panic::catch_unwind`. Declarations in an
//! `extern "C" { … }` block, `extern "C-unwind"` functions (where unwinding is
//! defined behaviour), and bodies with nothing that can panic are exempt.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-extern-c-without-catch-unwind",
    description: "A panic unwinding out of an `extern \"C\"` function aborts the whole process — the C caller never gets a chance to handle it.",
    remediation: "Wrap the body in `std::panic::catch_unwind(|| { … })` and turn an `Err` into the error code the C API already defines; or declare the function `extern \"C-unwind\"` when the caller is Rust and is prepared to unwind through it.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust", "security"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
