//! elysia-zod-coerce-params

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-zod-coerce-params",
    description: "`z.number()` / `z.boolean()` inside `params:` or `query:` Zod schema — Zod does not coerce strings.",
    remediation: "Use `z.coerce.number()` / `z.coerce.boolean()` for params/query because URL segments and query strings are always strings.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["validation", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
