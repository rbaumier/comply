//! ts-bounded-recursive-generic — recursive conditional/mapped types need
//! a depth accumulator.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-bounded-recursive-generic",
    description: "Recursive conditional or mapped type lacks a depth parameter; it can blow up the type checker.",
    remediation: "Add a depth accumulator (e.g. `D extends 0 ? ... : Recurse<Next<D>, ...>`) to bound recursion.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
