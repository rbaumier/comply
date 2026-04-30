//! auth-on-mutation

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "auth-on-mutation",
    description: "Mutation route handlers (POST/PUT/DELETE/PATCH) should reference auth.",
    remediation: "Add an auth check (`auth`, `token`, `session`, `middleware`, `guard`, `protect`, or `verify`) to mutation route handlers. Missing auth on mutations is a common security gap.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
