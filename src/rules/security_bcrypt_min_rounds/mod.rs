//! security-bcrypt-min-rounds

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "security-bcrypt-min-rounds",
    description: "`bcrypt.hash()` / `bcrypt.hashSync()` must use a cost factor of at least 12.",
    remediation: "Raise the second argument (cost / saltRounds) to 12 or higher to slow brute-force attacks.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
