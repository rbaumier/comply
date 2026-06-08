//! no-redundant-null-undefined-check — flag `x !== null && x !== undefined`
//! (and the `x === null || x === undefined` mirror).

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-redundant-null-undefined-check",
    description: "Comparing the same operand against both `null` and `undefined` is a \
                  verbose nullish check.",
    remediation: "Collapse `x !== null && x !== undefined` (or the \
                  `x === null || x === undefined` mirror) into a single strict, \
                  type-narrowing guard — not the loose `x != null`, which fights \
                  projects that ban `==`/`!=`. Define once: \
                  `function isDefined<T>(v: T): v is NonNullable<T> { return v !== null && v !== undefined; }`, \
                  then use `isDefined(x)` / `!isDefined(x)`. It narrows to \
                  `NonNullable<T>` (e.g. `arr.filter(isDefined)`). No autofix — the \
                  helper's name and location are project-specific.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],

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
