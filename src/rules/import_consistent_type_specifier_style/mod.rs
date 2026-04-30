//! import-consistent-type-specifier-style

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "import-consistent-type-specifier-style",
    description: "Type-only imports should use top-level `import type` syntax.",
    remediation: "Use `import type { Foo }` instead of `import { type Foo }`.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/import-js/eslint-plugin-import/blob/main/docs/rules/consistent-type-specifier-style.md",
    ),
    categories: &["imports"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
