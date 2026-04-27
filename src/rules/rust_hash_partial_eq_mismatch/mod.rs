//! rust-hash-partial-eq-mismatch — `Hash` and `PartialEq` mixed derive/manual.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-hash-partial-eq-mismatch",
    description: "`Hash` and `PartialEq` are not implemented consistently \
                  (one derived, the other manual).",
    remediation: "If two values are equal (`a == b`) they MUST produce the \
                  same hash. Mixing a derived `Hash` with a manual `PartialEq` \
                  (or vice versa) almost always breaks that invariant. Either \
                  derive both or implement both manually with care to keep \
                  them in sync.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
