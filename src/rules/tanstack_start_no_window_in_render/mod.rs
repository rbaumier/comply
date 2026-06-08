//! tanstack-start-no-window-in-render — forbid `window.*`/`document.*` in the
//! render body of route components (breaks SSR).

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-no-window-in-render",
    description: "`window.*` / `document.*` in render breaks SSR.",
    remediation: "Read from `window`/`document` inside a `useEffect` or behind \
                  a `typeof window !== 'undefined'` guard.",
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
