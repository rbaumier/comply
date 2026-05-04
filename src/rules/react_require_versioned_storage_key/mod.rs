//! react-require-versioned-storage-key — `localStorage.setItem("k", ...)` without `:vN` suffix.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-require-versioned-storage-key",
    description: "`localStorage.setItem` uses a literal key without a `:vN` version suffix, \
                  so a shape change to the stored value cannot be rolled forward.",
    remediation: "Add a version suffix (e.g. `\"settings:v1\"`) and bump it when the \
                  serialized shape changes so old entries can be migrated or dropped.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
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
