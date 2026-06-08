//! no-useless-error-capture-stack-trace — flag unnecessary Error.captureStackTrace() calls.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-useless-error-capture-stack-trace",
    description: "Unnecessary `Error.captureStackTrace()` in Error subclass constructor.",
    remediation: "Remove the `Error.captureStackTrace(this, ClassName)` call. \
                  Built-in Error subclasses already capture the stack trace \
                  automatically via `super()`.",
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
