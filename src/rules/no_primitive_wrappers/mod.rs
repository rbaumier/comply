//! no-primitive-wrappers

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-primitive-wrappers",
    description: "`new String()`, `new Number()`, `new Boolean()` create wrapper objects, not primitives.",
    remediation: "Use primitive literals or factory functions without `new`: `String(x)`, `Number(x)`, `Boolean(x)`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
