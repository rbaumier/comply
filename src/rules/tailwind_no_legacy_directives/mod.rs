//! tailwind-no-legacy-directives — forbid v3 `@tailwind` directives in CSS,
//! require the v4 `@import "tailwindcss"` form instead.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-legacy-directives",
    description: "Forbid `@tailwind base/components/utilities` (v3 syntax).",
    remediation: "Replace the three `@tailwind` directives with a single `@import \"tailwindcss\";` at the top of your entry stylesheet (Tailwind v4).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Css, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
