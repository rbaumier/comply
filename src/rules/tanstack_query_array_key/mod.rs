//! tanstack-query-array-key — query keys must be arrays.

#[cfg(test)]
mod typescript;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-array-key",
    description: "TanStack Query keys must be arrays, not strings.",
    remediation: "Wrap the string in brackets: `queryKey: ['todos']`. \
                  v5 requires arrays, and hierarchical invalidation \
                  (`invalidateQueries({ queryKey: ['todos'] })`) only \
                  works on array keys.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "tanstack"],

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
