//! factory-di-shape

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "factory-di-shape",
    description: "`create*` factory functions should take a single deps object, not individual params.",
    remediation: "Replace individual dependency parameters with a single object: `createService({ db, cache, logger })`. A deps object makes the dependency list extensible without breaking callers and reads as named arguments.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
