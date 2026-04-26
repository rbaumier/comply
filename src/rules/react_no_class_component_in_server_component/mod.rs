//! React class components rely on lifecycle methods and instance state that
//! exist only in the client runtime. A class component in a server component
//! file can't render, so flag it at authoring time.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-class-component-in-server-component",
    description: "Class components don't render in server components.",
    remediation: "Rewrite as a function component, or move the class into a \
                  `\"use client\"` module.",
    severity: Severity::Error,
    doc_url: Some("https://react.dev/reference/rsc/server-components"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
