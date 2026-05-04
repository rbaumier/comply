//! tailwind-no-off-scale-spacing — flag spacing utilities that fall
//! outside the conventional 4/8pt scale (e.g. `p-5`, `mb-7`, `gap-9`).
//! These odd values usually indicate pixel-pushing from a Figma export
//! rather than an intentional token choice and fragment the design system.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-off-scale-spacing",
    description: "Spacing values should stay on the 4/8pt scale (0, 1, 2, 4, 6, 8, 10, 12, 16…).",
    remediation: "Round to the nearest canonical scale step: `p-5` → `p-4` or `p-6`; `mb-7` → `mb-6` or `mb-8`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
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
