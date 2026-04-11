//! ts-no-dupe-class-members — disallow duplicate class members.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-dupe-class-members",
    description: "Duplicate class members shadow earlier definitions and indicate a bug.",
    remediation: "Remove or rename the duplicate class member. TS method overloads (without a body) are allowed.",
    severity: Severity::Error,
    doc_url: Some("https://typescript-eslint.io/rules/no-dupe-class-members"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
