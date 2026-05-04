//! react-no-this-in-sfc — `this.` inside a functional component.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-this-in-sfc",
    description: "`this` has no meaning inside a functional component.",
    remediation: "Remove `this.` references. Functional components don't have a \
                  `this` context — use hooks (`useState`, `useRef`, etc.) instead \
                  of `this.state`, `this.props`, etc.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
