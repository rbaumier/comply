//! no-unknown-property — flag HTML attributes on intrinsic JSX tags that are
//! not valid React props.
//!
//! React does not accept raw HTML attribute names like `class`, `for`,
//! `tabindex`, or lowercase event handlers. Using them silently breaks
//! styling and behavior. This rule matches intrinsic JSX tags (lowercase
//! elements) and flags known HTML-style names, suggesting the camelCase
//! React equivalent.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-unknown-property",
    description: "HTML attribute on an intrinsic JSX element is not a valid React prop.",
    remediation: "Replace the HTML attribute with its React camelCase equivalent \
                  (e.g. `class` → `className`, `for` → `htmlFor`, `onclick` → `onClick`).",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/no-unknown-property.md",
    ),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
