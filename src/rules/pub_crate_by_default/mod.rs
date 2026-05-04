mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "pub-crate-by-default",
    description: "`pub` item in a non-root module — prefer `pub(crate)` for internal items.",
    remediation: "Use `pub(crate)` or `pub(super)` instead of `pub` for items that are not part of the public API.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
