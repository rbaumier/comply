//! shadcn-dialog-requires-title — each `<DialogContent>` must contain
//! a `<DialogTitle>` descendant (Radix a11y requirement).

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "shadcn-dialog-requires-title",
    description: "`<DialogContent>` must render a `<DialogTitle>` for screen readers.",
    remediation: "Add a `<DialogTitle>` inside the dialog; use `VisuallyHidden` to keep it off-screen if needed.",
    severity: Severity::Error,
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
