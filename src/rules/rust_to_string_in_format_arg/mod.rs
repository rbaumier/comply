//! rust-to-string-in-format-arg — `format!("{}", x.to_string())` is redundant.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-to-string-in-format-arg",
    description: "`.to_string()` inside `format!` / `println!` / `write!` arguments is redundant.",
    remediation: "Drop the `.to_string()` — formatting macros already invoke \
                  `Display` (or the requested formatter) on each argument. \
                  The extra `.to_string()` allocates a `String` only to \
                  hand it back to the same trait.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
