//! rust-string-push-str-format — `s.push_str(&format!(...))` allocates a
//! temporary `String` only to copy it into another. `write!(s, ...)` writes
//! directly into the destination.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-string-push-str-format",
    description: "`s.push_str(&format!(...))` allocates a throwaway String — use `write!`.",
    remediation: "Replace `s.push_str(&format!(\"...\"))` with \
                  `write!(s, \"...\").unwrap()` (or `?` in a fallible \
                  context). `format!` allocates a `String` whose only \
                  purpose is to be copied into `s`; `write!` writes \
                  directly into `s` and avoids the round-trip allocation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust", "performance"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
