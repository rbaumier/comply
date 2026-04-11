//! ts-no-unsafe-declaration-merging — flag class/interface with same name.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-unsafe-declaration-merging",
    description: "Unsafe declaration merging between classes and interfaces.",
    remediation: "Rename one of the declarations so they don't merge.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
