//! boundary-condition — flag reads of array boundary elements (first or
//! last) without a guarding length check or a nullish fallback.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "boundary-condition",
    description: "Array boundary access (`arr[0]` or `arr[arr.length - 1]`) without a length guard or fallback.",
    remediation: "Guard the access with `if (arr.length)` / `arr.length > 0`, use `arr.at(0)` / `arr.at(-1)`, or provide a fallback via `?? fallback` or `|| fallback`. On an empty array, a raw boundary access returns `undefined` and will crash downstream code.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
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
