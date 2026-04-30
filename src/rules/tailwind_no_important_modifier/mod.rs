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
                Language::Vue,
                Backend::TreeSitter(Box::new(typescript::Check)),
            ),
        ],
    }
}
