//! ts-require-variance-annotation — exported generic interfaces need
//! `in`/`out` variance annotations.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-require-variance-annotation",
    description: "Generic parameters of exported interfaces should declare `in`/`out` variance.",
    remediation: "Annotate each type parameter with `in` (contravariant), `out` (covariant), or `in out` (invariant) so consumers can reason about subtyping.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
