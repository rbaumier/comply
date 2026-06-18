//! prefer-dom-node-remove
//!
//! Flags `parentNode.removeChild(childNode)` and suggests the DOM-only
//! `childNode.remove()`. Because `.remove()` exists solely on the DOM
//! `ChildNode` interface, the rule stays silent across the whole project when
//! any file declares a class method named `removeChild` — that marks a
//! project-owned tree type (HTML/XML AST, vdom, scene graph, …) whose nodes
//! have no `.remove()`, where the suggestion would throw at runtime.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-dom-node-remove",
    description: "Prefer `childNode.remove()` over `parentNode.removeChild(childNode)`.",
    remediation: "Replace `parent.removeChild(child)` with `child.remove()`. \
                  The modern `.remove()` API is simpler and doesn't require \
                  a reference to the parent node.",
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
