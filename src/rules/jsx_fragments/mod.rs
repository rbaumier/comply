//! jsx-fragments — enforce the short `<>...</>` syntax over
//! `<React.Fragment>...</React.Fragment>` (or bare `<Fragment>`).
//!
//! Why: the short syntax is terser, does not require importing
//! `Fragment`, and matches the idiomatic style in modern React code.
//! The long form is only necessary when a `key` prop is needed, which
//! the short syntax cannot express.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "jsx-fragments",
    description: "Prefer the short fragment syntax `<>...</>` over `<React.Fragment>`.",
    remediation: "Replace `<React.Fragment>` / `<Fragment>` with `<>...</>` (unless a `key` prop is required).",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/jsx-fragments.md",
    ),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
