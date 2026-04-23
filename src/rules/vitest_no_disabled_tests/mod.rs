//! vitest-no-disabled-tests — flag `xtest` / `xit` / `xdescribe` and `test.skip` variants.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "vitest-no-disabled-tests",
    description: "Disabled tests (`xtest`, `xit`, `xdescribe`, `.skip`) silently erode coverage.",
    remediation: "Re-enable the test, fix the underlying issue, or delete it if it's no longer relevant.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/vitest-dev/eslint-plugin-vitest/blob/main/docs/rules/no-disabled-tests.md"),
    categories: &["vitest"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
