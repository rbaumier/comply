//! no-thenable — flag objects/classes that define a `.then()` method.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-thenable",
    description: "Disallow `then` property on objects and classes.",
    remediation: "Rename the `then` method/property. Objects with a `then` \
                  method are treated as thenables by `await` and \
                  `Promise.resolve()`, causing unexpected behavior.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
