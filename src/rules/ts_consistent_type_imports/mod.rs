//! ts-consistent-type-imports — require `import type` for type-only imports.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-consistent-type-imports",
    description: "Type-only imports should use `import type` rather than `import`.",
    remediation: "Replace `import { Foo }` with `import type { Foo }` when only types are imported.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/consistent-type-imports/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
