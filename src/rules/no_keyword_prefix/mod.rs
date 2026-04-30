//! no-keyword-prefix — disallow identifiers starting with `new` or `class`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-keyword-prefix",
    description: "Do not prefix identifiers with keyword `new` or `class`.",
    remediation: "Rename the identifier to remove the keyword prefix. \
                  For example, `newUser` -> `user`, `classNames` -> `names` or `cssNames`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
