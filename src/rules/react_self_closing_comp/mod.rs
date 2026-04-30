//! react-self-closing-comp — components without children should self-close.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-self-closing-comp",
    description: "Components and HTML elements without children should use self-closing syntax.",
    remediation: "Replace `<Foo></Foo>` with `<Foo />` (and `<div></div>` with `<div />` \
                  in JSX). This reduces noise and makes it obvious the element has no content.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/self-closing-comp.md",
    ),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    let backends = crate::register_ts_family!(META, react).backends;
    RuleDef {
        meta: META,
        backends,
    }
}
