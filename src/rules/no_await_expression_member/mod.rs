//! no-await-expression-member — flag `(await expr).prop` / `(await expr)[0]`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-await-expression-member",
    description: "Do not access a member directly from an await expression.",
    remediation: "Extract the awaited value into a variable, then access the member: \
                  `const response = await fetch(url); const data = response.json();`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
