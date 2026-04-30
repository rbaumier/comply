//! prefer-string-raw

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-string-raw",
    description: "`String.raw` should be used to avoid escaping `\\`.",
    remediation: "Use `String.raw`\\`...\\`` for strings with multiple backslash escapes. \
                  This is clearer and avoids double-escaping mistakes.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
