//! prefer-string-starts-ends-with

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-string-starts-ends-with",
    description: "Prefer `String#startsWith()` / `String#endsWith()` over regex `^` / `$` tests.",
    remediation: "Replace `/^pattern/.test(str)` with `str.startsWith('pattern')` and \
                  `/pattern$/.test(str)` with `str.endsWith('pattern')`. \
                  String methods are faster and more readable than regex for simple prefix/suffix checks.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
