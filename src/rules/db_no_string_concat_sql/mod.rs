//! db-no-string-concat-sql

mod typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "db-no-string-concat-sql",
    description: "String concatenation with SQL keywords is a SQL injection vector.",
    remediation: "Use parameterized queries (`$1`, `?`, or ORM methods) instead of string concatenation. Never interpolate user input into SQL strings.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
