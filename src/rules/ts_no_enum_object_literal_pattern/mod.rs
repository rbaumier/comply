//! ts-no-enum-object-literal-pattern — `const X = { ... } as const` indexed
//! with an arbitrary string variable bypasses the type-narrowing the
//! `as const` was added for.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-enum-object-literal-pattern",
    description: "Indexing an `as const` enum-shaped object with an arbitrary string defeats the narrow type.",
    remediation: "Cast the index to `keyof typeof X` (`X[k as keyof typeof X]`), or convert the object \
                  to a real enum / discriminated map and accept the narrow keys explicitly.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],

    skip_in_test_dir: false,
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
