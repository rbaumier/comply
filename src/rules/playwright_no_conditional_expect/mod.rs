//! playwright-no-conditional-expect — flag `expect()` inside conditionals.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-conditional-expect",
    description: "`expect()` inside `if`/`switch`/`catch` may silently skip — tests must assert unconditionally.",
    remediation: "Move the `expect()` call out of the conditional branch. \
                  A conditional assertion can silently pass when the branch \
                  is never taken, giving false confidence. Structure the \
                  test so the expected state is deterministic.",
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
