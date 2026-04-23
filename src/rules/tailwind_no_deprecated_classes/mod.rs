//! tailwind-no-deprecated-classes

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-deprecated-classes",
    description: "Deprecated Tailwind v2/v3 utility classes should be replaced by their v3/v4 equivalents.",
    remediation: "Replace the deprecated utility with the listed replacement (e.g. `flex-grow-0` → `grow-0`, `overflow-ellipsis` → `text-ellipsis`).",
    severity: Severity::Warning,
    doc_url: Some("https://tailwindcss.com/docs/upgrade-guide"),
    categories: &["tailwind"],
};

pub fn register() -> RuleDef {
    let backends: Vec<_> = [
        Language::TypeScript,
        Language::Tsx,
        Language::JavaScript,
        Language::Vue,
    ]
    .into_iter()
    .map(|lang| (lang, Backend::Text(Box::new(text::Check))))
    .collect();
    RuleDef {
        meta: META,
        backends,
    }
}
