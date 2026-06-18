//! rust-partial-eq-without-eq — `PartialEq` derived/impl'd without `Eq`.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-partial-eq-without-eq",
    description: "Type derives `PartialEq` but not `Eq`.",
    remediation: "When every field type is provably `Eq` (non-float primitives, \
                  known-`Eq` stdlib types, or local Eq-capable types) the type \
                  should also derive `Eq`. `Eq` is a marker trait signalling \
                  reflexivity (`x == x`), and many APIs (`HashSet`, `BTreeMap` \
                  keys via wrapping) require it. A field whose `Eq`-ness cannot \
                  be proven — a float (`f32`/`f64`), an imported/unknown type, or \
                  a generic type parameter — leaves the type exempt, since adding \
                  `Eq` would not compile.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
