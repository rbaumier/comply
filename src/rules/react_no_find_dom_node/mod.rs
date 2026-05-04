//! react-no-find-dom-node — disallow `ReactDOM.findDOMNode()` and bare `findDOMNode()`.
//!
//! `findDOMNode` is deprecated in React 19 (removed from `react-dom`). It
//! breaks encapsulation, forces synchronous DOM access, and is incompatible
//! with future React rendering modes. Refs are the supported alternative.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-find-dom-node",
    description: "`findDOMNode` is deprecated in React 19 — use refs instead.",
    remediation: "Use refs instead of findDOMNode.",
    severity: Severity::Warning,
    doc_url: Some("https://react.dev/reference/react-dom/findDOMNode"),
    categories: &["react"],
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
