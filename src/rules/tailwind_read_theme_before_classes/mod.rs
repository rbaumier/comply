//! tailwind-read-theme-before-classes

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
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
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
