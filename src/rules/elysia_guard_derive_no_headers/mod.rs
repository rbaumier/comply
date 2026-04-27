//! elysia-guard-derive-no-headers

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-guard-derive-no-headers",
    description: "Guard `.derive`/`.resolve` reads `headers.authorization` but the guard has no `headers:` schema.",
    remediation: "Add a `headers: t.Object({ authorization: t.String() })` schema to the guard so the field is validated and typed.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
