//! no-and-in-function-name — flag function names like `getUserAndUpdateCache`.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-and-in-function-name",
    description: "`And` in a function name signals two responsibilities — split it.",
    remediation: "A function with `And` in its name does two things. Split into \
                  two functions named after each responsibility, then let the caller \
                  compose them: `getUserAndUpdateCache` → `getUser()` + `updateCache(user)`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["naming"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
