//! no-mutation

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-mutation",
    description: "Disallow calling an in-place mutating method (`push`, `sort`, `Object.assign`, …) on a `const`-bound value.",
    remediation: "Use the non-mutating counterpart (`[...arr, x]`, `toSorted()`, `{ ...a, ...b }`) and bind the result, or lift the change up to the producer.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["functional"],

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
