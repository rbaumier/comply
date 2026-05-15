//! tailwind-no-restricted-classes — flag classnames matching a configurable blocklist.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-restricted-classes",
    description: "Configurable blocklist of Tailwind classes — typically used to ban legacy spacing tokens, ad-hoc colors, or deprecated utility names.",
    remediation: "Use the project-approved equivalent. If the class is needed for a one-off, escape via the project's design-token override mechanism.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

/// Classes blocked by default — a sensible starting point for projects
/// adopting OKLCH-only palettes and the `space-*` design system.
pub(crate) const DEFAULT_BLOCKLIST: &[&str] = &[
    // !important shortcut.
    "!important",
    // Black/white as raw color (no semantic meaning).
    "text-black",
    "text-white",
    "bg-black",
    "bg-white",
    // Legacy `space-*` directional classes (favour `gap-*` instead).
    "space-y-px",
    "space-x-px",
];

pub(crate) fn class_is_blocked(class: &str) -> Option<&'static str> {
    DEFAULT_BLOCKLIST
        .iter()
        .find(|blocked| **blocked == class)
        .copied()
}

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
