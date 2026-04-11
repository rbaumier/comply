//! a11y-html-has-lang

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-html-has-lang",
    description: "The `<html>` element must have a `lang` attribute.",
    remediation: "Add `lang=\"en\"` (or the appropriate language code) to the `<html>` element.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
