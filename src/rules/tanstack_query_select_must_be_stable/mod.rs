//! tanstack-query-select-must-be-stable — flag inline `select:` arrow
//! functions in `useQuery` options. A new function reference each render
//! defeats `select`'s memoization; wrap with `useCallback`.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-select-must-be-stable",
    description: "Inline `select:` arrow rebuilds each render and re-runs the selector.",
    remediation: "Wrap the selector with `useCallback` or hoist it to module scope.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::Text(Box::new(typescript::Check))),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
