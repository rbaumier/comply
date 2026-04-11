//! ts-ban-tslint-comment — disallow `// tslint:<rule-flag>` comments.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-ban-tslint-comment",
    description: "TSLint comments are obsolete — the project has been deprecated in favour of ESLint.",
    remediation: "Remove the `tslint:` comment directive.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/ban-tslint-comment/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
