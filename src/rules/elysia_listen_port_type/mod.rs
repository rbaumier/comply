//! elysia-listen-port-type

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-listen-port-type",
    description: "`process.env.PORT` is a string — `.listen()` expects a number.",
    remediation: "Wrap with `Number(process.env.PORT)`, `parseInt(process.env.PORT, 10)`, or default with `?? 3000`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
