//! elysia-set-status-after-return

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-set-status-after-return",
    description: "`set.status = ...` written after a `return` is dead code — Elysia has already serialized the response.",
    remediation: "Set `set.status` before the `return` statement that emits the response body.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
