//! regex-no-obscure-range

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-obscure-range",
    description: "Character class ranges like `[A-z]` include unwanted chars (`[\\]^_\\``). Use `[A-Za-z]` instead.",
    remediation: "Replace obscure ranges with explicit ones: `[A-Za-z]`, `[a-zA-Z0-9]`, etc.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
