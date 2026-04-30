//! react-no-find-in-map-loop — `.find()`/`.filter()` nested inside `.map()` or a `for` loop.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-find-in-map-loop",
    description: "`.find()` / `.filter()` called inside a `.map()` callback or `for` loop \
                  turns an O(n) pass into O(n²).",
    remediation: "Build a `Map`/lookup index once, then look up inside the loop.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react", "code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
