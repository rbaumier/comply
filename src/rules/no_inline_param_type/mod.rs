//! no-inline-param-type — extract object-shaped parameter types to named types.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-inline-param-type",
    description: "Inline object types in parameters resist reuse and refactoring.",
    remediation: "Extract the inline type to a named `type` declaration \
                  above the function. A named type has an identity, can be \
                  shared across call sites, and shows up in IDE hover.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Oxc(Box::new(oxc_typescript::Check))))
            .collect(),
    }
}
