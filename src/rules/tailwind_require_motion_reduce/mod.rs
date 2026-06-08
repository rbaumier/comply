//! tailwind-require-motion-reduce — require `motion-reduce:*` on any
//! element that uses a `transition-*` or `animate-*` utility, so users
//! who set `prefers-reduced-motion: reduce` aren't forced to watch
//! animations they opted out of.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-require-motion-reduce",
    description: "Elements with `transition-*` / `animate-*` must also declare a `motion-reduce:*` variant.",
    remediation: "Add `motion-reduce:transition-none` (or `motion-reduce:animate-none`) so users with `prefers-reduced-motion: reduce` are respected.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind", "a11y"],

    skip_in_test_dir: true,
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
