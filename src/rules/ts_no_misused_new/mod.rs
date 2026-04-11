//! ts-no-misused-new — flag `new()` in classes and `constructor()` in interfaces.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-misused-new",
    description: "Classes use `constructor()`, not `new()`. Interfaces use `new()`, not `constructor()`.",
    remediation: "In a class, rename `new` to `constructor`. In an interface, use `new(): Type` \
                  instead of `constructor(): Type`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
