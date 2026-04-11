//! no-misleading-collection-name — flag `*List` named as `Set`/`Map`/etc.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-misleading-collection-name",
    description: "Variable name lies about the underlying collection type.",
    remediation: "Rename the binding to match the actual type — `userList` holding \
                  a `Set` becomes `userSet`, `nameMap` holding an `Array` becomes \
                  `nameList`. The name and the type must agree.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["naming"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
