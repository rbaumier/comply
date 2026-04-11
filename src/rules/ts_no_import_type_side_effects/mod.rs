//! ts-no-import-type-side-effects — enforce top-level `import type` when
//! all specifiers use inline `type` qualifiers.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-import-type-side-effects",
    description: "Inline `type` qualifiers on every specifier leave a side-effect import at runtime.",
    remediation: "Use a top-level `import type { ... }` instead of `import { type A, type B }`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-import-type-side-effects/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
