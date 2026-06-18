//! ts-no-empty-object-type — flag `{}` used as a type.
//!
//! Targets annotation and declaration positions, where `{}` matches any
//! non-nullish value rather than the empty object the author meant. Exempts the
//! positions where `{}` is deliberate: a generic constraint/default
//! (`T extends {}`, `T = {}`), an intersection identity (`T & {}`) whose other
//! operand is a non-empty type, and an explicit type argument (`Foo<{}>`,
//! `Component<{}>`, `TaggedError("x")<{}>()`) — instantiating a generic with `{}`
//! fills a slot the API designed for an empty payload. Skipped entirely in test
//! directories, where `{}` is the expected type under type-level assertions
//! (`expectType<{}>(...)`) rather than a value annotation.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-empty-object-type",
    description: "`{}` as a type matches any non-nullish value — it almost never means what you think.",
    remediation: "Use `Record<string, never>` for an empty object, `object` for any object, \
                  or `unknown` for any value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],

    skip_in_test_dir: true,
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
