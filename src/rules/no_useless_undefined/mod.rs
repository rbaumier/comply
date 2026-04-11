//! no-useless-undefined

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-useless-undefined",
    description: "Disallow useless `undefined`.",
    remediation: "Remove the explicit `undefined` — JavaScript already defaults \
                  to it in `return`, `let`/`var` initializers, and default parameter values.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
