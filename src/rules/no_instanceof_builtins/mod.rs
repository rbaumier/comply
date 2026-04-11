//! no-instanceof-builtins — flag `x instanceof Array` and other builtins.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-instanceof-builtins",
    description: "Avoid `instanceof` for built-in types — it fails across realms.",
    remediation: "Use `Array.isArray(x)` instead of `x instanceof Array`. \
                  For errors, check the `name` property or use `Error.isError()`. \
                  `instanceof` breaks across iframes, VMs, and module boundaries.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
