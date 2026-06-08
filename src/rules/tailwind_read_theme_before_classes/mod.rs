//! tailwind-read-theme-before-classes

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-read-theme-before-classes",
    description: "Arbitrary Tailwind values (`p-[13px]`, `bg-[#abc]`) are used without \
                  the file referencing `tailwind.config` / `resolveConfig` / `theme(...)`.",
    remediation: "Either switch to a design-token class (`p-4`, `bg-brand`) or import the \
                  theme via `resolveConfig(tailwindConfig)` / `theme('spacing.4')` so the \
                  arbitrary value stays in sync with the config.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],

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
