//! react-no-ref-read-during-render — reading `ref.current` during render is
//! a footgun: refs are designed to be read inside event handlers and
//! effects, not during the render pass.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-ref-read-during-render",
    description: "Reading `ref.current` during render is unstable — refs are not designed for the render pass.",
    remediation: "Read `ref.current` inside an event handler, `useEffect`, or `useLayoutEffect`. \
                  If you need a value during render, use state instead of a ref.",
    severity: Severity::Warning,
    doc_url: Some("https://react.dev/learn/referencing-values-with-refs"),
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
