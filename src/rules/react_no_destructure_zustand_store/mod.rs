//! react-no-destructure-zustand-store — whole-store destructuring of a zustand hook.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-destructure-zustand-store",
    description: "Destructuring the full zustand store (`const { x, y } = useStore()`) \
                  subscribes the component to every state change.",
    remediation: "Use a selector per field: `const x = useStore(s => s.x)` so the \
                  component only re-renders when that slice changes.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
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
