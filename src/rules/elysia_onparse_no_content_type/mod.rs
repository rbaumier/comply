//! elysia-onparse-no-content-type

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-onparse-no-content-type",
    description: "`.onParse` handler does not branch on `contentType`.",
    remediation: "Inspect `contentType` inside `onParse` and only return a parsed value for the formats this hook handles; otherwise let Elysia's default parsing run.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
