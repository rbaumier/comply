//! no-parameters-only-used-in-recursion — parameters passed through recursive calls only.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-parameters-only-used-in-recursion",
    description: "A function parameter is only ever forwarded to recursive calls to the same function, never read otherwise, so it carries no information into the computation.",
    remediation: "Remove the parameter (and the argument at every call site) or replace it with a constant; prefix it with `_` if the recursion genuinely needs the slot.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["suspicious"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    let backends = vec![
        (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
    ];
    RuleDef {
        meta: META,
        backends,
    }
}
