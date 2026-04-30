//! ts-triple-slash-reference — disallow `/// <reference ... />` directives.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-triple-slash-reference",
    description: "Triple-slash reference directives are legacy — use ES `import` instead.",
    remediation: "Replace `/// <reference path=\"...\" />` or `/// <reference types=\"...\" />` with an ES `import` declaration.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/triple-slash-reference"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
