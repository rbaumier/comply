//! tailwind-enforce-consistent-important-position — `!` always prefix.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-enforce-consistent-important-position",
    description: "Mixing `!class` and `class!` for the !important variant is inconsistent and harder to grep for.",
    remediation: "Pick the prefix form (`!text-red-500`) — it's the documented Tailwind v4 syntax — and use it everywhere.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

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
