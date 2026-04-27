//! rust-ord-partial-ord-inconsistent — `Ord` and `PartialOrd` mixed derive/manual.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-ord-partial-ord-inconsistent",
    description: "`Ord` and `PartialOrd` are not implemented consistently \
                  (one derived, the other manual).",
    remediation: "If both traits are present, the contract requires \
                  `partial_cmp(a, b) == Some(cmp(a, b))`. Mixing a derived \
                  `Ord` with a manual `PartialOrd` (or vice versa) typically \
                  desyncs them. Either derive both or implement both manually \
                  with care to keep them aligned.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
