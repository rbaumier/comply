//! vitest-no-disabled-tests — flag `xtest` / `xit` / `xdescribe` and `test.skip` variants.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vitest-no-disabled-tests",
    description: "Disabled tests (`xtest`, `xit`, `xdescribe`, `.skip`) silently erode coverage.",
    remediation: "Re-enable the test, fix the underlying issue, or delete it if it's no longer relevant.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/vitest-dev/eslint-plugin-vitest/blob/main/docs/rules/no-disabled-tests.md",
    ),
    categories: &["vitest"],

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
