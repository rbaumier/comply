//! Bans while/do-while loops — prefer recursion or higher-order functions.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-while-loop",
    description: "Bans while/do-while loops.",
    remediation: "Use recursion, Array methods, or generators instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["functional"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
