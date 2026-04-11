//! prefer-top-level-await — flag async IIFE and `async main(); main()` patterns.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-top-level-await",
    description: "Prefer top-level await over async IIFE or async-function-then-call patterns.",
    remediation: "Use top-level `await` directly instead of wrapping in an async IIFE \
                  or defining an async function and immediately calling it. \
                  Top-level await is supported in ESM.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
