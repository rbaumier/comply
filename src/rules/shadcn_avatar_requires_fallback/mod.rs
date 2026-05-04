//! shadcn-avatar-requires-fallback — every `<Avatar>` must render an
//! `<AvatarFallback>` so the UI never renders a broken image.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "shadcn-avatar-requires-fallback",
    description: "`<Avatar>` must contain an `<AvatarFallback>` so broken images degrade gracefully.",
    remediation: "Add an `<AvatarFallback>` child (initials, icon, or empty circle) alongside `<AvatarImage>`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["shadcn", "a11y"],
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
