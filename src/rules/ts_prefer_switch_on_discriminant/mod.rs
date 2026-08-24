//! ts-prefer-switch-on-discriminant — hand-rolled narrowing of a tagged union.
//!
//! A `"kind" in x` test or a chain of `x.kind === "…"` tests reads the tag the
//! union already carries, and neither shape tells the compiler which variants
//! were covered. A `switch` closed by `assertNever` does.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-prefer-switch-on-discriminant",
    description: "A tagged union should be narrowed by a `switch` on its discriminant, not by `in` or an if/else chain.",
    remediation: "Switch on the discriminant property and close the switch with \
                  `assertNever` (or `const _exhaustive: never = x;`). The compiler \
                  then reports a variant nobody handled; `\"kind\" in x` and repeated \
                  `x.kind === \"…\"` tests report nothing.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        // TS-family only: exhaustiveness is a type-checker verdict, so a plain
        // `.js`/`.mjs` file gains nothing from the rewrite the rule asks for.
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
