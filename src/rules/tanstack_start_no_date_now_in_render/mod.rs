//! tanstack-start-no-date-now-in-render — forbid `Date.now()`, `new Date()`,
//! `Math.random()` in the render body of route components (hydration mismatch).

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-no-date-now-in-render",
    description: "`Date.now()`, `new Date()`, `Math.random()` in render cause \
                  hydration mismatches.",
    remediation: "Compute non-deterministic values inside a `useEffect`, a \
                  loader, or a server function.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-start", "react"],

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
