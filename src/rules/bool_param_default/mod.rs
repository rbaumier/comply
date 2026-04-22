//! bool-param-default — optional boolean parameters must have an explicit default.
//!
//! SonarJS S4798. An optional boolean parameter without a default leaves call
//! sites ambiguous: `connect("api")` with `secure?: boolean` silently passes
//! `undefined`, and readers can't tell whether the omitted argument means
//! `true` or `false`. Giving it a default (`secure: boolean = true`) makes
//! the intended behavior part of the signature.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "bool-param-default",
    description: "Optional boolean parameters should declare an explicit default value.",
    remediation: "Replace `x?: boolean` with `x: boolean = <default>` (or `x = <default>`) \
                  so call sites that omit the argument have an unambiguous behavior. \
                  Prefer this over reading the body to guess what `undefined` means.",
    severity: Severity::Warning,
    doc_url: Some("https://sonarsource.github.io/rspec/#/rspec/S4798"),
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
