//! vitest-hoisted-apis-on-top — `vi.mock` / `vi.hoisted` must appear before imports
//! (Vitest hoists them automatically, but placing them after imports hides that fact).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "vitest-hoisted-apis-on-top",
    description: "`vi.mock` / `vi.hoisted` are hoisted above imports — placing them after imports misleads readers.",
    remediation: "Move `vi.mock(...)` / `vi.hoisted(...)` calls to the top of the file, above all `import` statements.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/vitest-dev/eslint-plugin-vitest/blob/main/docs/rules/prefer-hoisted.md"),
    categories: &["vitest"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
