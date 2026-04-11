//! react-no-this-in-sfc — `this.` inside a functional component.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

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
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
