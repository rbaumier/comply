//! react-no-danger-with-children — dangerouslySetInnerHTML + children.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "react-no-danger-with-children",
    description: "Using both `dangerouslySetInnerHTML` and `children` on the same element is invalid.",
    remediation: "Use either `dangerouslySetInnerHTML` OR `children`, not both. \
                  React will throw a runtime error when both are provided on \
                  the same element.",
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
