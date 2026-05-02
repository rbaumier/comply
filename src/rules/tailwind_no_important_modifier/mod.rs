mod oxc_typescript;

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-important-modifier",
    description: "The Tailwind `!` important modifier signals a specificity fight, not a real fix.",
    remediation: "Fix the specificity issue instead of using `!` — restructure class order or use a more specific selector.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

/// True when `s` contains a `!` immediately followed by an ASCII lowercase
/// letter — the Tailwind important-modifier shape (`!text-red-500`).
pub(crate) fn has_important_class(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes
        .windows(2)
        .any(|w| w[0] == b'!' && w[1].is_ascii_lowercase())
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
