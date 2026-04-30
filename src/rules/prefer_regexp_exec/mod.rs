//! prefer-regexp-exec

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-regexp-exec",
    description: "`.match(/regex/)` is slower than `regex.exec(string)` for non-global regexps.",
    remediation: "Use `regex.exec(string)` instead of `string.match(regex)`. For non-global regular expressions, `RegExp.prototype.exec()` is faster and returns the same result.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
