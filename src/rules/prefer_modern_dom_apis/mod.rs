//! prefer-modern-dom-apis

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-modern-dom-apis",
    description: "Prefer `.before()` / `.replaceWith()` over `.insertBefore()` / `.replaceChild()`.",
    remediation: "Replace `parent.insertBefore(newNode, ref)` with `ref.before(newNode)` \
                  and `parent.replaceChild(newNode, old)` with `old.replaceWith(newNode)`. \
                  The modern APIs are called on the target node directly, removing the \
                  need for a parent reference.",
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
