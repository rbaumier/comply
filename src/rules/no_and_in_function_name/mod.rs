//! no-and-in-function-name — flag function names like `getUserAndUpdateCache`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-and-in-function-name",
    description: "`And` in a function name signals two responsibilities — split it.",
    remediation: "A function with `And` in its name does two things. Split into \
                  two functions named after each responsibility, then let the caller \
                  compose them: `getUserAndUpdateCache` → `getUser()` + `updateCache(user)`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["naming"],

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
