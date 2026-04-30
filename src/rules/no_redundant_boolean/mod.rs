//! no-redundant-boolean

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-redundant-boolean",
    description: "Redundant boolean literal in a return or condition.",
    remediation: "Simplify: `if (x) return true; else return false;` \u{2192} `return x;`. `x === true` \u{2192} `x`. The boolean adds no information.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
