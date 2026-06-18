//! prefer-modern-math-apis — prefer modern `Math` APIs over legacy patterns.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-modern-math-apis",
    description: "Prefer modern `Math` APIs: `Math.hypot()`, `Math.log2()`, `Math.log10()`.",
    remediation: "Replace `Math.sqrt(a*a + b*b)` with `Math.hypot(a, b)`, \
                  `Math.log(x) / Math.LN2` with `Math.log2(x)`, \
                  `Math.log(x) * Math.LOG2E` with `Math.log2(x)`, \
                  `Math.log(x) / Math.LN10` with `Math.log10(x)`, \
                  `Math.log(x) * Math.LOG10E` with `Math.log10(x)`.",
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
