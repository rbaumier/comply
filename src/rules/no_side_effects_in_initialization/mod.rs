//! no-side-effects-in-initialization — flag top-level side effects in
//! modules so bundlers can safely tree-shake them.
//!
//! Bare function calls, `new` expressions, and IIFEs at the module's top
//! level execute as soon as the module is imported, which prevents
//! bundlers from eliminating the module even when none of its exports are
//! used. Expressions annotated with `/*#__PURE__*/` are honored and
//! skipped: that marker is the bundler-wide convention for declaring a
//! call as side-effect-free.
//!
//! Declarations (const/let/var/function/class) and imports are fine —
//! only raw `expression_statement`s containing a call or `new` at module
//! scope are flagged.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-side-effects-in-initialization",
    description: "Top-level side effects (bare calls, `new`, IIFE) at module \
                  scope prevent tree-shaking.",
    remediation: "Avoid side effects at module top-level for tree-shaking",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/lukastaegert/eslint-plugin-tree-shaking"),
    categories: &["code-quality"],

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
