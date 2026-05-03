//! ui-no-side-tab-border — `borderLeft` / `borderRight` combined with a
//! `borderBottom` indicator in the same inline style block is the classic
//! "tab" pattern. The side borders are decorative noise — the bottom border
//! is what reads as the active-tab indicator.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-side-tab-border",
    description: "`borderLeft` / `borderRight` alongside a `borderBottom` indicator on a tab — \
                  the side borders compete with the active-tab affordance.",
    remediation: "Drop the side borders and keep only the `borderBottom` (or use a dedicated \
                  active-state indicator).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],
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
