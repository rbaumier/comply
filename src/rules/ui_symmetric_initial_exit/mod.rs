//! ui-symmetric-initial-exit — `initial` and `exit` on a `motion.*`
//! component should share the same key set so the animation mirrors.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-symmetric-initial-exit",
    description: "`initial` and `exit` props on a `motion.*` component should share the same keys so enter/exit feel mirrored.",
    remediation: "Make `exit` declare the same properties as `initial` (e.g. both set `opacity` + `y`).",
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
