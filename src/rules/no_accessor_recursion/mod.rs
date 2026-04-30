//! no-accessor-recursion — flag getters/setters that access their own property on `this`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-accessor-recursion",
    description: "Disallow recursive access in getters and setters.",
    remediation: "A getter that reads `this.foo` or a setter that writes \
                  `this.foo` on the same property triggers infinite recursion. \
                  Use a backing field (e.g. `this._foo`) or a `WeakMap`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
