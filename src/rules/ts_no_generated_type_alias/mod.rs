//! ts-no-generated-type-alias — flag a type alias that renames, or derives with
//! `Pick`/`Omit`/`Partial`, a type imported from a generated module.
//!
//! The generator owns the contract, so a second name for it is a copy that the
//! next regeneration invalidates on one side only. Use sites read the copy and
//! believe they read the contract, so the divergence lands at runtime instead of
//! at the type check.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-generated-type-alias",
    description: "An alias or a `Pick`/`Omit`/`Partial` derivative of a generated type copies a contract the generator owns.",
    remediation: "Consume the generated type directly at every use site; when the shape you need is missing, add it where the generator reads from.",
    severity: Severity::Error,
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
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
