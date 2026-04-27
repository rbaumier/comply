//! elysia-no-context-type

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-no-context-type",
    description: "Function parameter typed as `Context` from elysia — manual typing breaks Elysia's inferred context.",
    remediation: "Let Elysia infer the context type. Destructure what you need: `({ body, set, store }) => ...`",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["type-safety", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
