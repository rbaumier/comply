//! prefer-global-this — prefer `globalThis` over `window`, `self`, and `global`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-global-this",
    description: "Prefer `globalThis` over `window`, `self`, and `global`.",
    remediation: "Replace `window.`, `self.`, or `global.` with `globalThis.`. \
                  `globalThis` is the standard cross-platform way to access the \
                  global object in any JS environment.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
