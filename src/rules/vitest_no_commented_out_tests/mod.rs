//! vitest-no-commented-out-tests — commented-out `it()` / `test()`.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vitest-no-commented-out-tests",
    description: "Commented-out `test(...)` / `it(...)` / `describe(...)` is dead code with mock value.",
    remediation: "Delete the commented test, or move it back behind `.skip` if it's a known-failing case worth tracking.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing", "vitest"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
