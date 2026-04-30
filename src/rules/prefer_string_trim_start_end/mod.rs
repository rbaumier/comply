//! prefer-string-trim-start-end

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-string-trim-start-end",
    description: "Prefer `String#trimStart()` / `String#trimEnd()` over the deprecated `trimLeft()` / `trimRight()`.",
    remediation: "Replace `.trimLeft()` with `.trimStart()` and `.trimRight()` with `.trimEnd()`. \
                  The `trimLeft`/`trimRight` aliases are deprecated in favor of the spec names.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
