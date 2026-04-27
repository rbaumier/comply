//! elysia-nextjs-typeof-process

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-nextjs-typeof-process",
    description: "Eden treaty isomorphic clients must branch on `typeof process` — `typeof window` is unreliable in RSC / edge runtimes.",
    remediation: "Use `typeof process !== 'undefined'` to detect the server side when configuring the treaty client.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
