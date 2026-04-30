//! rust-repeated-string-concat-in-loop — building a `String` by `push_str`
//! or `+` inside a hot loop reallocates and copies on every iteration.
//! Pre-size with `String::with_capacity` or `Vec<String>::join`.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-repeated-string-concat-in-loop",
    description: "`String` concatenation inside a loop reallocates per iteration.",
    remediation: "Pre-size with `String::with_capacity(estimate)` and \
                  `push_str` into it, or collect into `Vec<String>` and \
                  use `.join(\"\")`. The bare `s = s + …` form drops and \
                  re-allocates on every iteration.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust", "performance"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
