//! prefer-string-starts-ends-with

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

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
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
