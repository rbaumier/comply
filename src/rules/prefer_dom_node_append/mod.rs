//! prefer-dom-node-append
//!
//! Flags `parentNode.appendChild(childNode)` and suggests the DOM-only
//! `parentNode.append(childNode)`. Because `.append()` exists solely on the DOM
//! `ParentNode` interface, the rule stays silent across the whole project when
//! any file declares a class method named `appendChild` — that marks a
//! project-owned tree type (HTML/XML AST, vdom, scene graph, …) whose nodes
//! have no `.append()`, where the suggestion would throw at runtime.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-dom-node-append",
    description: "Prefer `Node#append()` over `Node#appendChild()`.",
    remediation: "Replace `.appendChild(x)` with `.append(x)`. \
                  `.append()` accepts multiple arguments, strings, and \
                  never returns the appended node (avoiding subtle misuse).",
    severity: Severity::Error,
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
