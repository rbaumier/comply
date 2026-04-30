//! jsx-no-undef — flag JSX component tags that aren't imported or declared
//! in the current file. Lowercase tags (HTML intrinsics), fragments and
//! member expressions (`<Foo.Bar />`) are skipped.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "jsx-no-undef",
    description: "JSX component tag refers to an undefined identifier.",
    remediation: "Import the component or declare it in this file before using it in JSX.",
    severity: Severity::Error,
    doc_url: Some(
        "https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/jsx-no-undef.md",
    ),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
