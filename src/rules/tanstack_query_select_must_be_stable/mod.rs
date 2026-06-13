//! tanstack-query-select-must-be-stable — flag inline `select:` arrow
//! functions in `useQuery` options. A new function reference each render
//! defeats `select`'s memoization; wrap with `useCallback`.

#[cfg(test)]
mod typescript;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-select-must-be-stable",
    description: "Inline `select:` arrow rebuilds each render and re-runs the selector.",
    remediation: "Wrap the selector with `useCallback` or hoist it to module scope.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
