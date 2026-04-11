//! react-self-closing-comp — components without children should self-close.

mod text;
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-self-closing-comp",
    description: "Components and HTML elements without children should use self-closing syntax.",
    remediation: "Replace `<Foo></Foo>` with `<Foo />` (and `<div></div>` with `<div />` \
                  in JSX). This reduces noise and makes it obvious the element has no content.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/self-closing-comp.md"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    let mut backends = crate::register_ts_family!(META, typescript).backends;
    backends.push((Language::Vue, Backend::Text(Box::new(text::Check))));
    RuleDef { meta: META, backends }
}
