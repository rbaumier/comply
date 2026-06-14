//! next-metadata-missing-viewport

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-metadata-missing-viewport",
    description: "Layout files exporting `metadata` should also export `viewport` (inherited by nested pages).",
    remediation: "Add `export const viewport: Viewport = { width: 'device-width', initialScale: 1 };` next to your `metadata` export.",
    severity: Severity::Warning,
    doc_url: Some("https://nextjs.org/docs/app/api-reference/functions/generate-viewport"),
    categories: &["nextjs", "a11y"],
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
