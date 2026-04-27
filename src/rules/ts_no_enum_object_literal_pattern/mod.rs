//! ts-no-enum-object-literal-pattern — `const X = { ... } as const` indexed
//! with an arbitrary string variable bypasses the type-narrowing the
//! `as const` was added for.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-enum-object-literal-pattern",
    description: "Indexing an `as const` enum-shaped object with an arbitrary string defeats the narrow type.",
    remediation: "Cast the index to `keyof typeof X` (`X[k as keyof typeof X]`), or convert the object \
                  to a real enum / discriminated map and accept the narrow keys explicitly.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
