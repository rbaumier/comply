mod oxc_typescript;

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

/// Deprecated class → recommended replacement.
pub(crate) const DEPRECATED: &[(&str, &str)] = &[
    ("flex-grow-0", "grow-0"),
    ("flex-grow", "grow"),
    ("flex-shrink-0", "shrink-0"),
    ("flex-shrink", "shrink"),
    ("overflow-ellipsis", "text-ellipsis"),
    ("overflow-clip", "text-clip"),
    ("decoration-slice", "box-decoration-slice"),
    ("decoration-clone", "box-decoration-clone"),
];

pub(crate) fn replacement_for(class: &str) -> Option<&'static str> {
    DEPRECATED
        .iter()
        .find(|(dep, _)| *dep == class)
        .map(|(_, repl)| *repl)
}

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::Tsx,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::Vue,
                Backend::TreeSitter(Box::new(typescript::Check)),
            ),
        ],
    }
}
