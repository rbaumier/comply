//! react-no-leaked-fetch — `fetch(...)` in `useEffect` without an
//! AbortController signal cannot be cancelled on unmount.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-leaked-fetch",
    description: "`fetch(...)` in `useEffect` without an AbortController signal cannot be cancelled on unmount.",
    remediation: "Create an AbortController, pass its `signal` to `fetch`, and return a cleanup that calls `controller.abort()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
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
