mod typescript;
use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-serializable-key",
    description: "Query keys must be structurally serializable — no functions, Dates, Symbols, or class instances.",
    remediation: "Serialize the value first: use `date.toISOString()` instead of `new Date()`, a string tag instead of a Symbol, and a plain identifier instead of a closure or class instance.",
    severity: Severity::Error,
    doc_url: Some("https://tanstack.com/query/v5/docs/react/guides/query-keys"),
    categories: &["tanstack-query"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
