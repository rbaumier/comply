//! no-unenclosed-multiline-block

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-unenclosed-multiline-block",
    description: "`if`/`for`/`while` without braces and a multiline body is a bug magnet.",
    remediation: "Always wrap `if`/`for`/`while` bodies in curly braces `{}` when the body is on the next line.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
