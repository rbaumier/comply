//! no-manual-rtl-cleanup — `cleanup()` is automatic with Vitest.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-manual-rtl-cleanup",
    description: "Importing `cleanup` from `@testing-library/react` in Vitest causes double cleanup.",
    remediation: "Remove the `cleanup` import and any `afterEach(cleanup)` \
                  call. Vitest with `@testing-library/react` runs cleanup \
                  automatically after each test. Manual cleanup causes \
                  double cleanup which can mask unmount bugs.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],

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
