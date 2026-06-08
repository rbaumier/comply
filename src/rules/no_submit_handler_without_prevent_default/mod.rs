//! no-submit-handler-without-preventDefault — require `onSubmit` JSX handlers
//! to call `event.preventDefault()`.
//!
//! Without `preventDefault`, the browser performs a full-page navigation on
//! submit, wiping any client-side state. Controlled React forms almost always
//! want this call. The rule is scoped to inline arrow / function expressions
//! so it does not false-positive on handlers defined elsewhere.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-submit-handler-without-preventDefault",
    description: "Inline `onSubmit={...}` handler does not call `preventDefault()`.",
    remediation: "Call `event.preventDefault()` at the top of the handler, or switch to a form action that doesn't navigate.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],

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
