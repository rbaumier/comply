//! ts-no-mixed-decorator-systems — standard and experimentalDecorators
//! cannot coexist in the same file.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-mixed-decorator-systems",
    description: "File mixes standard decorators with `reflect-metadata`/experimentalDecorators usage.",
    remediation: "Pick one decorator system per file — remove the `reflect-metadata` import or the standard decorator, or split the code across two files.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
