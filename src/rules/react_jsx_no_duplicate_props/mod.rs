//! react-jsx-no-duplicate-props — duplicate props in JSX.

mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-no-duplicate-props",
    description: "Duplicate props in JSX — the last one silently wins.",
    remediation: "Remove the duplicate prop. When the same prop name appears \
                  multiple times on a JSX element, only the last value takes \
                  effect, which is almost always a copy-paste mistake.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::Tsx, Backend::TreeSitter(Box::new(react::Check))),
        ],
    }
}
