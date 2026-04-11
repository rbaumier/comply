//! react-no-unstable-nested-components — component defined inside render.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "react-no-unstable-nested-components",
    description: "Component defined inside another component causes unmount/remount every render.",
    remediation: "Move the inner component outside the parent component. Defining a \
                  component inside render means React sees a brand-new type on every \
                  render, destroying the entire subtree's DOM nodes and state.",
    severity: Severity::Warning,
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
