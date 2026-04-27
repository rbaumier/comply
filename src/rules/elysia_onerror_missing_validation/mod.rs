//! elysia-onerror-missing-validation

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-onerror-missing-validation",
    description: "`onError` handler doesn't branch on `'VALIDATION'` — schema errors will be returned as generic 500s.",
    remediation: "Inside `onError`, branch on `code === 'VALIDATION'` (or `'NOT_FOUND'`/`'PARSE'`) and return a structured response.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
