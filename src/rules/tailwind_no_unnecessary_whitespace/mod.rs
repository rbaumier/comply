mod oxc_typescript;

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-unnecessary-whitespace",
    description: "Multiple consecutive spaces in className/class attributes are unnecessary.",
    remediation: "Remove extra whitespace in class strings",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

/// True when `s` contains two or more consecutive space characters.
pub(crate) fn has_consecutive_spaces(s: &str) -> bool {
    s.as_bytes()
        .windows(2)
        .any(|w| w[0] == b' ' && w[1] == b' ')
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
