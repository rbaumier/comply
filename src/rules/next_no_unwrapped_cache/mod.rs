//! next-no-unwrapped-cache

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-unwrapped-cache",
    description: "`unstable_cache` callbacks must handle errors — an unhandled throw poisons the cache.",
    remediation: "Wrap the inner work in try/catch and return a sentinel, or guard the call site with an error boundary.",
    severity: Severity::Warning,
    doc_url: Some("https://nextjs.org/docs/app/api-reference/functions/unstable_cache"),
    categories: &["nextjs", "reliability"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
