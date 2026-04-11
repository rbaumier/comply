//! ts-no-wrapper-object-types — flag `String`, `Number`, `Boolean`, etc. in type positions.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-wrapper-object-types",
    description: "Use lowercase primitives (`string`, `number`, `boolean`) instead of wrapper object types.",
    remediation: "Replace `String` with `string`, `Number` with `number`, `Boolean` with `boolean`, \
                  `Object` with `object`, `Symbol` with `symbol`, `BigInt` with `bigint`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
        ],
    }
}
