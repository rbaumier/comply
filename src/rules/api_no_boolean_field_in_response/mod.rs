//! api-no-boolean-field-in-response — flag `boolean` properties in
//! interfaces/type aliases that name an API response shape.
//!
//! Booleans lock the contract into a two-state world. The moment a third
//! state appears (`pending`, `archived`, `scheduled`, ...), clients must
//! juggle two fields (`isActive` + `isArchived`) or the server breaks
//! existing consumers by flipping semantics. An enum / string-union
//! starts at two states and grows without breaking the wire format.
//!
//! Scope: only response-shaped names — `*Response`, `*Dto`, `*Payload`,
//! `*Reply`, `*Result`, `*Body`. Internal models and request types are
//! untouched.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "api-no-boolean-field-in-response",
    description: "Response types should use string-unions / enums instead of booleans for extensibility.",
    remediation: "Replace `isActive: boolean` with `status: 'active' | 'inactive' | ...` so new states don't break the wire contract.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
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
