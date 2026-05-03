//! perf-route-level-code-split — route components (imported from a `pages/`,
//! `routes/`, or `views/` path) should be loaded via `React.lazy(() => import(...))`
//! so the router only ships the bundle the user actually navigates to.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "perf-route-level-code-split",
    description: "Route components must be imported via `React.lazy()` / dynamic `import()`, not a static `import`.",
    remediation: "Replace `import Foo from './pages/Foo'` with `const Foo = React.lazy(() => import('./pages/Foo'))`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["web-performance", "react"],
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
