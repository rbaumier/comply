//! react-no-object-in-dep-array — flag non-primitive values in hook
//! dependency arrays.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-object-in-dep-array",
    description: "Hook dep arrays must not contain values that allocate every render.",
    remediation: "Move inline object/array literals, inline functions, and \
                  `new Map()`-style allocations out of the dep array. Extract \
                  them into `useMemo`/`useCallback`, or depend on primitive \
                  fields that are stable across renders.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "react"],

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
