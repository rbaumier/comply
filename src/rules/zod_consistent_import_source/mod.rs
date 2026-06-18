//! zod-consistent-import-source — flag imports from non-standard zod subpaths,
//! while allowing the official versioned entry points (`zod/v3`, `zod/v4`, …).

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-consistent-import-source",
    description: "Imports from non-standard zod subpaths (e.g., `zod/src/...`, `zod/dist/...`) \
                  circumvent the public API and cause inconsistent schemas across the codebase. \
                  Official versioned entry points (`zod/v3`, `zod/v4`, `zod/v4-mini`) are allowed.",
    remediation: "Use consistent import source for zod",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],

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
