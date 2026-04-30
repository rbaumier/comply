mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-deprecated-classes",
    description: "Deprecated Tailwind v2/v3 utility classes should be replaced by their v3/v4 equivalents.",
    remediation: "Replace the deprecated utility with the listed replacement (e.g. `flex-grow-0` → `grow-0`, `overflow-ellipsis` → `text-ellipsis`).",
    severity: Severity::Warning,
    doc_url: Some("https://tailwindcss.com/docs/upgrade-guide"),
    categories: &["tailwind"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::TreeSitter(Box::new(typescript::Check)),
            ),
            (
                Language::Tsx,
                Backend::TreeSitter(Box::new(typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::TreeSitter(Box::new(typescript::Check)),
            ),
            (
                Language::Vue,
                Backend::TreeSitter(Box::new(typescript::Check)),
            ),
        ],
    }
}
