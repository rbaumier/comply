//! no-invalid-void-type — ports typescript-eslint's
//! `@typescript-eslint/no-invalid-void-type`: `void` is only meaningful
//! as a return type or inside a generic constraint; flag it anywhere else.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-invalid-void-type",
    description: "`void` used outside of a return type or a generic type argument.",
    remediation: "Use `undefined` for a value, or restrict `void` to return-type positions \
                  and generic parameters where it conveys 'no useful value'.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-invalid-void-type"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
