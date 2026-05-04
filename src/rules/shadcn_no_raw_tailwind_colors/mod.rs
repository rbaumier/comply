//! shadcn-no-raw-tailwind-colors — flag raw Tailwind color utilities
//! (e.g. `bg-blue-500`, `text-gray-600`) in JSX `className` values and
//! require shadcn semantic tokens (`bg-primary`, `text-muted-foreground`).

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "shadcn-no-raw-tailwind-colors",
    description: "Raw Tailwind color utilities break shadcn theming — use semantic tokens instead.",
    remediation: "Replace `bg-blue-500`/`text-gray-600` with `bg-primary`/`text-muted-foreground` and the theme variables they resolve to.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["shadcn", "tailwind"],
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
