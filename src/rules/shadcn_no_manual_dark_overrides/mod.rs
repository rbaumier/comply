//! shadcn-no-manual-dark-overrides — flag `dark:bg-*` / `dark:text-*`
//! etc. paired with explicit light-mode colors; shadcn's semantic tokens
//! already theme-switch automatically.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "shadcn-no-manual-dark-overrides",
    description: "Manual `dark:` color overrides reintroduce the duplication shadcn tokens eliminate.",
    remediation: "Replace the light/dark pair (e.g. `bg-white dark:bg-gray-900`) with a semantic token like `bg-background`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["shadcn", "tailwind"],

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
