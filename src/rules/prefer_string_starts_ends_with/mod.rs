//! prefer-string-starts-ends-with

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-string-starts-ends-with",
    description: "Prefer `String#startsWith()` / `String#endsWith()` over regex `^` / `$` tests.",
    remediation: "Replace `/^pattern/.test(str)` with `str.startsWith('pattern')` and \
                  `/pattern$/.test(str)` with `str.endsWith('pattern')`. \
                  String methods are faster and more readable than regex for simple prefix/suffix checks.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
