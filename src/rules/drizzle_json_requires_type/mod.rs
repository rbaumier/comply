//! drizzle-json-requires-type

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-json-requires-type",
    description: "`json()`/`jsonb()` columns without `.$type<T>()` infer as `unknown`.",
    remediation: "Call `.$type<T>()` on every `json()`/`jsonb()` column so queries return a typed shape instead of `unknown`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
