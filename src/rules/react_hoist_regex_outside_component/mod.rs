//! react-hoist-regex-outside-component — compile regex once.

mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-hoist-regex-outside-component",
    description: "Regex literals inside components are recompiled every render.",
    remediation: "Move the regex to a module-level `const` above the \
                  component. Regex literals inside a function body allocate \
                  a new RegExp object every call, defeating the JS engine's \
                  compiled-pattern cache.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "react"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Tsx, Backend::TreeSitter(Box::new(react::Check)))],
    }
}
