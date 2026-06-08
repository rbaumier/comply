//! shadcn-sheet-requires-title — each `<SheetContent>` must contain
//! a `<SheetTitle>` descendant (Radix a11y requirement).

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "shadcn-sheet-requires-title",
    description: "`<SheetContent>` must render a `<SheetTitle>` for screen readers.",
    remediation: "Add a `<SheetTitle>` inside the sheet; use `VisuallyHidden` to keep it off-screen if needed.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["shadcn", "a11y"],

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
