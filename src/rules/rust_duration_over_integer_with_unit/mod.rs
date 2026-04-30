//! rust-duration-over-integer-with-unit — prefer `std::time::Duration`
//! over raw integers whose name encodes a time unit.
//!
//! Flags struct fields and function parameters named `*_seconds`,
//! `*_ms`, `*_millis`, `*_milliseconds`, `*_minutes`, `*_hours`,
//! `*_days`, `*_nanos`, `*_nanoseconds`, `*_micros`, `*_microseconds`
//! (and common short forms like `_secs`, `_mins`, `_hrs`, `_ns`, `_us`)
//! whose type is a primitive integer or float. Encoding a unit in the
//! name but not the type makes `window_days: u32` and `timeout_ms: u64`
//! interchangeable at the call site — a `Duration` makes that impossible.
//!
//! False negatives: type paths (`std::time::Duration`), wrappers
//! (`Option<Duration>`, `Arc<Duration>`), and type aliases are not
//! flagged because we compare the raw type text against a small set
//! of primitive numeric types. That's a deliberate tradeoff — we'd
//! rather miss an edge case than shout at `timeout_ms: Option<u64>`
//! where the user already handles `None`.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-duration-over-integer-with-unit",
    description: "Prefer `Duration` over integers whose name encodes a time unit.",
    remediation: "Replace `window_days: u32` with `window: std::time::Duration`. \
                  For config parsing, use a humantime crate or similar to accept \
                  \"30d\" / \"24h\" at the config boundary. The type then carries \
                  the unit through the codebase.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
