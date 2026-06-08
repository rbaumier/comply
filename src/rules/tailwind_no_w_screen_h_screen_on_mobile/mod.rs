//! tailwind-no-w-screen-h-screen-on-mobile — `w-screen` / `h-screen`
//! use the visual viewport, which on mobile differs from the layout
//! viewport when the URL bar collapses, causing layout jumps.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-w-screen-h-screen-on-mobile",
    description: "`w-screen` / `h-screen` cause layout jumps when the mobile URL bar collapses.",
    remediation: "Use `w-full` / `min-h-dvh` (dynamic viewport units) instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind", "mobile"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
