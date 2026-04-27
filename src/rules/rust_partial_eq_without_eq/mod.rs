//! rust-partial-eq-without-eq — `PartialEq` derived/impl'd without `Eq`.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-partial-eq-without-eq",
    description: "Type derives `PartialEq` but not `Eq`.",
    remediation: "If your type doesn't contain floats (or other partial-only \
                  types) it should also derive `Eq`. `Eq` is a marker trait \
                  signalling reflexivity (`x == x`), and many APIs (`HashSet`, \
                  `BTreeMap` keys via wrapping) require it. If the type \
                  intentionally has only partial equality (contains `f32`/`f64`), \
                  add a comment explaining why.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
