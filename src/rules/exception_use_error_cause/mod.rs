//! exception-use-error-cause — flag re-throws of `new Error(...)` without
//! `{ cause }` inside a `catch` block.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "exception-use-error-cause",
    description: "Rethrowing a new Error from catch without `{ cause }` drops the original stack.",
    remediation: "When wrapping a caught error in a new one, pass `{ cause: e }` \
                  as the second argument: `throw new Error('context', { cause: e })`. \
                  Otherwise the original stack trace and error chain are lost.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["error-handling"],

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
