//! no-evolving-types — disallow variables that implicitly evolve into `any`.
//!
//! In TypeScript, a `let`/`var`/`const` binding with neither a type annotation
//! nor an initializer (`let a;`), or initialized to `null` or an empty array
//! (`let c = null;`, `const b = [];`) with no annotation, has an *evolving*
//! implicit type. Under a relaxed `noImplicitAny`, later assignments widen it
//! toward `any`, silently disabling type checking. The fix is an explicit type
//! annotation or a concrete initializer.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-evolving-types",
    description: "Variable may implicitly evolve into the `any` type.",
    remediation: "Add an explicit type annotation or a concrete initializer to pin the type.",
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
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
