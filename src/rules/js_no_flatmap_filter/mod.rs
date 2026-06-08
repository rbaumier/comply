//! js-no-flatmap-filter — `.flatMap(...).filter(...)` iterates twice;
//! return an empty array from `flatMap` to combine map+filter in one pass.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "js-no-flatmap-filter",
    description: "`.flatMap().filter()` iterates twice — combine into a single `.flatMap()`.",
    remediation: "Move the filter predicate into the `flatMap` callback and return `[]` for \
                  items that should be excluded.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance"],

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
