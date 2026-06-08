//! tanstack-query-stable-client — `new QueryClient()` inside a component
//! creates a fresh cache on every render.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-stable-client",
    description: "`new QueryClient()` inside a component recreates the cache every render.",
    remediation: "Hoist `new QueryClient()` to module scope, or wrap it in \
                  `useState(() => new QueryClient())` / `useRef`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-query"],

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
