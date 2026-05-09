//! no-abbreviated-names — reject usr/btn/cfg/ctx/msg and similar.

mod oxc_typescript;
mod rust;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-abbreviated-names",
    description: "Identifier contains a banned abbreviation.",
    remediation: "Use the full word: `usr` → `user`, `btn` → `button`. \
                  Add project-specific bans via `banned = [\"mgr:manager\"]` \
                  in comply.toml. Editors auto-complete; readers don't.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["naming"],
};
pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}
