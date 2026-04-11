//! no-negation-in-equality-check — flag `!x === y` (precedence bug).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-negation-in-equality-check",
    description: "Negated expression in equality check is a precedence bug.",
    remediation: "`!x === y` is parsed as `(!x) === y`, not `!(x === y)`. \
                  Use `x !== y` or wrap explicitly: `!(x === y)`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
