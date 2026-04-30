//! no-abbreviated-names — reject usr/btn/cfg/ctx/msg and similar.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-abbreviated-names",
    description: "Identifier contains a banned abbreviation.",
    remediation: "Use the full word: `usr` → `user`, `cfg` → `config`, \
                  `btn` → `button`. Editors auto-complete; readers don't.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["naming"],
};
pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
