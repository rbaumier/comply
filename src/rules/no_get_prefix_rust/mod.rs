mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-get-prefix-rust",
    description: "Simple accessor uses `get_` prefix — Rust convention is to omit it.",
    remediation: "Rename `get_foo(&self)` to `foo(&self)`. Reserve `get` for fallible or expensive operations.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
