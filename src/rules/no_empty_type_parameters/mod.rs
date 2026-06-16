//! no-empty-type-parameters — flag an empty type-parameter list `<>` on a
//! type alias (`type Foo<> = ...`) or interface (`interface Bar<> {}`).
//!
//! An empty `<>` declares a generic with no parameters: it is meaningless
//! and confusing. Either remove the angle brackets or add a real type
//! parameter. A non-empty list (`<T>`) or an absent list is accepted.
//!
//! Port of Biome's `noEmptyTypeParameters`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-empty-type-parameters",
    description: "Type aliases and interfaces must not declare an empty type-parameter list `<>`.",
    remediation: "Remove the empty `<>`, or add a type parameter such as `<T>`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["complexity"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
